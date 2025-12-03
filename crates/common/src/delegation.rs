use crate::constants::{AO_AUTHORITY, ARWEAVE_GATEWAY, DELEGATION_PID};
use crate::projects::PI_PID;
use anyhow::{Error, anyhow};
use serde_json::{Value, json};

pub(crate) const DELEGATION_PID_START_HEIGHT: u32 = 1_608_145;

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
        .ok_or(anyhow!("error: error accessing user delegation msg id"))?;

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
        .ok_or(anyhow!("error: error accessing last delegation msg id"))
        .unwrap_or(PI_PID);

    Ok(id.to_string())
}

/// Action : Delegation-Mappings
pub fn get_delegation_mappings(first: Option<u32>) -> Result<String, Error> {
    let template = r#"
query GetDetailedTransactions {
  transactions(
    first: $firstvar
    sort: HEIGHT_DESC
    owners: ["$addressvar"]
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

    let query = template
        .replace("$addressvar", AO_AUTHORITY)
        .replace("$firstvar", &first.unwrap_or(1).to_string());

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
        .ok_or(anyhow!("error: error accessing delegation mappings msg id"))?;

    Ok(id.to_string())
}
