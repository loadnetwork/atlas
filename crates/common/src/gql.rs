use crate::constants::{
    AO_AUTHORITY, ARWEAVE_GATEWAY, DAI_ORACLE_PID, STETH_ORACLE_PID, USDS_ORACLE_PID,
};
use anyhow::{Error, anyhow};
use serde_json::{Value, json};

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Oracle {
    USDS,
    DAI,
    STETH,
    Unknown,
}

impl Oracle {
    pub fn resolve(&self) -> String {
        match self {
            &Oracle::USDS => USDS_ORACLE_PID.to_string(),
            &Oracle::DAI => DAI_ORACLE_PID.to_string(),
            &Oracle::STETH => STETH_ORACLE_PID.to_string(),
            &Oracle::Unknown => String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OracleStakers {
    pub oracle: Oracle,
    query: Option<Value>,
    server_resp: Option<Value>,
}

impl OracleStakers {
    pub fn new(oracle: &str) -> Self {
        match oracle.to_ascii_lowercase().as_str() {
            "usds" => OracleStakers {
                oracle: Oracle::USDS,
                query: None,
                server_resp: None,
            },
            "dai" => OracleStakers {
                oracle: Oracle::DAI,
                query: None,
                server_resp: None,
            },
            "steth" => OracleStakers {
                oracle: Oracle::STETH,
                query: None,
                server_resp: None,
            },
            _ => OracleStakers {
                oracle: Oracle::Unknown,
                query: None,
                server_resp: None,
            },
        }
    }

    pub fn build(&mut self) -> Result<&mut Self, Error> {
        if self.oracle == Oracle::Unknown {
            return Err(anyhow!("error: unknown oracle type"));
        };

        let template = r#"
            query GetDetailedTransactions($owner: String!, $oracle: String!) {
    transactions(
        first: 1
        sort: HEIGHT_DESC
        owners: [$owner]
        tags: [
        { name: "Action", values: ["Set-Balances"] },
        {name: "From-Process", values: [$oracle]}
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
            .replace("$owner", AO_AUTHORITY)
            .replace("$oracle", &self.oracle.resolve());

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

    pub fn send(&self) -> Result<Value, Error> {
        let url = format!("{ARWEAVE_GATEWAY}/graphql");
        let req = ureq::post(url)
            .send_json(self.query.clone())?
            .body_mut()
            .read_to_string()?;
        let res: Value = serde_json::from_str(&req)?;
        Ok(res)
    }
    pub fn id(&self) -> Result<String, Error> {
        let res = self.server_resp.clone().ok_or(anyhow!(
            "error: no gql server response was made successfully"
        ))?;
        let msg_id = res
            .get("data")
            .and_then(|v| v.get("transactions"))
            .and_then(|v| v.get("edges"))
            .and_then(|v| v.get(0))
            .and_then(|v| v.get("node"))
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .ok_or(anyhow!("error: no ao message id found for the given query"))?;

        Ok(msg_id.to_string())
    }
}
