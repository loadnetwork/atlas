use crate::constants::{AO_AUTHORITY, DELEGATION_PID, arweave_gateway};
use crate::projects::INTERNAL_PI_PID;
use anyhow::{Error, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const DELEGATION_PID_START_HEIGHT: u32 = 1_608_145;

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

    let req = ureq::post(format!("{}/graphql", arweave_gateway()))
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
        .ok_or(anyhow!("error: error accessing user delegation msg id"))?;

    Ok(id.to_string())
}

pub fn get_user_last_delegation_txid(address: &str) -> Result<Vec<String>, Error> {
    let template = r#"
    query GetDetailedTransactions {
  transactions(
    first: 10
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

    let req = ureq::post(format!("{}/graphql", arweave_gateway()))
        .send_json(body)?
        .body_mut()
        .read_to_string()?;
    let res: Value = serde_json::from_str(&req)?;

    let edges = res
        .get("data")
        .and_then(|v| v.get("transactions"))
        .and_then(|v| v.get("edges"))
        .and_then(|v| v.as_array());

    if let Some(edges) = edges {
        let mut nodes = Vec::new();
        for edge in edges {
            let Some(node) = edge.get("node") else {
                continue;
            };
            let Some(id) = node.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let height = node
                .get("block")
                .and_then(|v| v.get("height"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            nodes.push((id.to_string(), height));
        }
        if nodes.is_empty() {
            return Ok(vec![INTERNAL_PI_PID.to_string()]);
        }
        let max_height = nodes.iter().map(|(_, h)| *h).max().unwrap_or(0);
        let ids: Vec<String> = nodes
            .into_iter()
            .filter(|(_, h)| *h == max_height)
            .map(|(id, _)| id)
            .collect();
        if ids.is_empty() {
            return Ok(vec![INTERNAL_PI_PID.to_string()]);
        }
        return Ok(ids);
    }
    Ok(vec![INTERNAL_PI_PID.to_string()])
}

/// Action : Delegation-Mappings
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DelegationMappingMeta {
    pub tx_id: String,
    pub height: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DelegationMappingsPage {
    pub mappings: Vec<DelegationMappingMeta>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

pub fn get_delegation_mappings(
    first: Option<u32>,
    after: Option<&str>,
) -> Result<DelegationMappingsPage, Error> {
    let first = first.unwrap_or(1).to_string();
    let template = r#"
query GetDetailedTransactions {
  transactions(
    first: $firstvar
    sort: HEIGHT_DESC
    owners: ["$addressvar"]
$afterclause
    tags: [
      { name: "Action", values: ["Delegation-Mappings"] }
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

    let after_clause = after
        .map(|cursor| format!("    after: \"{cursor}\"\n"))
        .unwrap_or_default();
    let query = template
        .replace("$addressvar", AO_AUTHORITY)
        .replace("$firstvar", &first)
        .replace("$afterclause", &after_clause);

    let body = json!({
        "query": query,
        "variables": {}
    });

    let req = ureq::post(format!("{}/graphql", arweave_gateway()))
        .send_json(body)?
        .body_mut()
        .read_to_string()?;
    let res: Value = serde_json::from_str(&req)?;

    let txs = res
        .get("data")
        .and_then(|v| v.get("transactions"))
        .ok_or(anyhow!(
            "error: no transactions object found for the delegation mappings query"
        ))?;
    let has_next_page = txs
        .get("pageInfo")
        .and_then(|v| v.get("hasNextPage"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let edges = txs.get("edges").and_then(|v| v.as_array()).ok_or(anyhow!(
        "error: no ao message edges found for the delegation mappings query"
    ))?;
    let mut out = Vec::new();
    let mut last_cursor = None;
    for edge in edges {
        if let Some(cursor) = edge.get("cursor").and_then(|v| v.as_str()) {
            last_cursor = Some(cursor.to_string());
        }
        let Some(node) = edge.get("node") else {
            continue;
        };
        let Some(id) = node.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let height = node
            .get("block")
            .and_then(|v| v.get("height"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(0);
        out.push(DelegationMappingMeta {
            tx_id: id.to_string(),
            height,
        });
    }

    if out.is_empty() {
        return Err(anyhow!("error: no ao message id found for the given query"));
    }
    Ok(DelegationMappingsPage {
        mappings: out,
        has_next_page,
        end_cursor: last_cursor,
    })
}

#[cfg(test)]
mod tests {
    use crate::delegation::get_delegation_mappings;

    #[test]
    fn get_latest_delegation_mappings_test() {
        let res = get_delegation_mappings(None, None).unwrap();
        println!("{:?}", res);
        assert_eq!(res.has_next_page, true);
    }
}
