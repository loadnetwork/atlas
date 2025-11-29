use crate::constants::{
    AO_AUTHORITY, ARWEAVE_GATEWAY, DAI_ORACLE_PID, DELEGATION_PID, STETH_ORACLE_PID,
    USDS_ORACLE_PID,
};
use crate::projects::PI_PID;
use anyhow::{Error, anyhow};
use serde_json::{Value, json};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Oracle {
    USDS,
    DAI,
    STETH,
    All,
    Unknown,
}

impl Oracle {
    pub fn resolve(&self) -> String {
        match self {
            &Oracle::USDS => format!("[\"{USDS_ORACLE_PID}\"]"),
            &Oracle::DAI => format!("[\"{DAI_ORACLE_PID}\"]"),
            &Oracle::STETH => format!("[\"{STETH_ORACLE_PID}\"]"),
            &Oracle::All => {
                format!("[\"{USDS_ORACLE_PID}\", \"{DAI_ORACLE_PID}\", \"{STETH_ORACLE_PID}\"]")
            }
            &Oracle::Unknown => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OracleStakers {
    pub oracle: Oracle,
    query: Option<Value>,
    server_resp: Option<Value>,
    last_updates: Option<Vec<String>>,
}

impl OracleStakers {
    pub fn new(oracle: &str) -> Self {
        match oracle.to_ascii_lowercase().as_str() {
            "usds" => OracleStakers {
                oracle: Oracle::USDS,
                query: None,
                server_resp: None,
                last_updates: None,
            },
            "dai" => OracleStakers {
                oracle: Oracle::DAI,
                query: None,
                server_resp: None,
                last_updates: None,
            },
            "steth" => OracleStakers {
                oracle: Oracle::STETH,
                query: None,
                server_resp: None,
                last_updates: None,
            },
            "all" => OracleStakers {
                oracle: Oracle::All,
                query: None,
                server_resp: None,
                last_updates: None,
            },
            _ => OracleStakers {
                oracle: Oracle::Unknown,
                query: None,
                server_resp: None,
                last_updates: None,
            },
        }
    }

    pub fn build(mut self) -> Result<Self, Error> {
        if self.oracle == Oracle::Unknown {
            return Err(anyhow!("error: unknown oracle type"));
        };

        let first_var = if self.oracle != Oracle::All { 1 } else { 3 };

        let template = r#"
            query GetDetailedTransactions {
    transactions(
        first: $firstvar
        sort: HEIGHT_DESC
        owners: ["$ownervar"]
        tags: [
        { name: "Action", values: ["Set-Balances"] },
        {name: "From-Process", values: $oraclevar }
        ]
    ) {
        edges {
        cursor
        node {
            id
            owner {
            address
            }
            tags {
            name
            value
            }
            block {
            id
            height
            }
        }
        }
        pageInfo {
        hasNextPage
        }
    }
    }
        "#;

        // formatting as arweave.net doesnt support dynamic vars on server level
        let query = template
            .replace("$firstvar", &first_var.to_string())
            .replace("$ownervar", AO_AUTHORITY)
            .replace("$oraclevar", &self.oracle.resolve());

        let vars = json!({
            "owner": AO_AUTHORITY,
            "oracle": self.oracle.resolve()
        });

        let body = json!({
            "query": query,
            "variables": vars // ignored on server level but kept for future compatibility with other gateways
        });

        self.query = Some(body);

        Ok(self)
    }

    pub fn send(mut self) -> Result<Self, Error> {
        let url = format!("{ARWEAVE_GATEWAY}/graphql");
        let req = ureq::post(url)
            .send_json(self.query.clone())?
            .body_mut()
            .read_to_string()?;
        let res: Value = serde_json::from_str(&req)?;
        self.server_resp = Some(res);
        Ok(self)
    }
    pub fn last_updates(mut self) -> Result<Vec<String>, Error> {
        if self.last_updates.is_none() {
            self.set_last_updates()?;
        }

        self.last_updates
            .clone()
            .ok_or(anyhow!("error while retrieving the message id"))
    }

    pub fn last_update(mut self) -> Result<String, Error> {
        if self.last_updates.is_none() {
            self.set_last_updates()?;
        }

        self.last_updates
            .clone()
            .ok_or(anyhow!("error while retrieving the message id"))?
            .get(0)
            .ok_or(anyhow!("error while retrieving the message id"))
            .cloned()
    }
    fn set_last_updates(&mut self) -> Result<Vec<String>, Error> {
        if self.last_updates.is_some() {
            return Err(anyhow!("error: message id is already set"));
        };
        let res = self.server_resp.clone().ok_or(anyhow!(
            "error: no gql server response was made successfully"
        ))?;
        let edges = res
            .get("data")
            .and_then(|v| v.get("transactions"))
            .and_then(|v| v.get("edges"))
            .and_then(|v| v.as_array())
            .ok_or(anyhow!(
                "error: no ao message edges found for the given query"
            ))?;

        let ids: Vec<String> = edges
            .iter()
            .filter_map(|edge| {
                edge.get("node")
                    .and_then(|node| node.get("id"))
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
            })
            .collect();

        if ids.is_empty() {
            return Err(anyhow!("error: no ao message id found for the given query"));
        }

        self.last_updates = Some(ids.clone());
        Ok(ids)
    }
}

pub fn get_user_delegation_txid(last_delegation_txid: &str) -> Result<String, Error> {
    let template = r#"
    query GetDetailedTransactions {
  transactions(
    first: 1
    sort: HEIGHT_DESC
    owners: ["$addressvar"]
    tags: [
      { name: "From-Process", values: ["$delegationpidvar"] },
      { name: "Pushed-For", values: ["$lastdelegationvar"] }
    ]
  ) {
    edges {
      cursor
      node {
        id
        owner {
          address
        }
        tags {
          name
          value
        }
        block {
          id
          height
        }
      }
    }
    pageInfo {
      hasNextPage
    }
  }
}
    "#;

    let query = template
        .replace("$addressvar", AO_AUTHORITY)
        .replace("$delegationpidvar", DELEGATION_PID)
        .replace("$lastdelegationvar", last_delegation_txid);

    let body = json!({
        "query": query,
        "variables": {}
    });

    let req = ureq::post(format!("{ARWEAVE_GATEWAY}/graphql"))
        .send_json(body)?
        .body_mut()
        .read_to_string()?;
    let res: Value = serde_json::from_str(&req)?;

    let id = res
        .get("data")
        .and_then(|v| v.get("transactions"))
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("node"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .ok_or(anyhow!("error: error accessing delegation msg id"))?;

    Ok(id.to_string())
}

pub fn get_user_last_delegation_txid(address: &str) -> Result<String, Error> {
    let template = r#"
    query GetDetailedTransactions {
  transactions(
    first: 1
    sort: HEIGHT_DESC
    owners: ["$addressvar"]
    tags: [
      { name: "Action", values: ["Set-Delegation"] }
    ]
  ) {
    edges {
      cursor
      node {
        id
        owner {
          address
        }
        tags {
          name
          value
        }
        block {
          id
          height
        }
      }
    }
    pageInfo {
      hasNextPage
    }
  }
}
    "#;

    let query = template.replace("$addressvar", address);

    let body = json!({
        "query": query,
        "variables": {}
    });

    let req = ureq::post(format!("{ARWEAVE_GATEWAY}/graphql"))
        .send_json(body)?
        .body_mut()
        .read_to_string()?;
    let res: Value = serde_json::from_str(&req)?;

    let id = res
        .get("data")
        .and_then(|v| v.get("transactions"))
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("node"))
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        // default to the PI address as if a wallet has no Set-Delegation message record
        // the FLP bridge system default for 100% of delegation preference to $PI
        .ok_or(anyhow!("error: error accessing delegation msg id"))
        .unwrap_or(PI_PID);

    Ok(id.to_string())
}

#[cfg(test)]
mod test {
    use crate::gql::{OracleStakers, get_user_delegation_txid, get_user_last_delegation_txid};
    #[test]
    fn test_single_oracle_usds_stakers() {
        let oracle = OracleStakers::new("steth").build().unwrap().send().unwrap();
        let id = oracle.last_update().unwrap();
        println!("ORACLE LAST UPDATE: {:?}", id);
        assert_eq!(id.len(), 43);
    }
    #[test]
    fn test_all_oracle_stakers() {
        let oracle = OracleStakers::new("all").build().unwrap().send().unwrap();
        // noticied arweave gql gateway behavior is returning IDs in this order:
        // USDS / STETH / DAI
        let id = oracle.last_updates().unwrap();
        println!("ORACLE LAST UPDATES: {:?}", id);
        assert_eq!(id.len(), 3);
    }
    #[test]
    fn test_get_user_deleation() {
        let address = "vZY2XY1RD9HIfWi8ift-1_DnHLDadZMWrufSh-_rKF0";
        let last_id = get_user_last_delegation_txid(address).unwrap();
        println!("LAST DELEGATION ID {last_id}");
        let delegation_id = get_user_delegation_txid(&last_id).unwrap();
        println!("DELEGATION ID {delegation_id}");
        assert_eq!(last_id.len(), delegation_id.len());
    }
}
