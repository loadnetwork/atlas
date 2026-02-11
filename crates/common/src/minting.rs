use crate::constants::{AO_AUTHORITY, arweave_gateway};
use anyhow::{Error, anyhow};
use serde_json::{Value, json};

/// Action : Add-Own-Mint-Report
pub fn get_flp_own_minting_report(flp_id: &str) -> Result<String, Error> {
    let template = r#"
    query GetDetailedTransactions {
  transactions(
    first: 1
    sort: HEIGHT_DESC
    owners: ["$addressvar"]
    tags: [
      { name: "Action", values: ["Add-Own-Mint-Report"] },
      { name: "From-Process", values: ["$flpidvar"] }
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
        .replace("$flpidvar", flp_id);

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
        .ok_or(anyhow!(
            "error: error accessing flp's last minting cycle report msg id"
        ))?;

    Ok(id.to_string())
}

#[cfg(test)]

mod tests {
    use crate::minting::get_flp_own_minting_report;
    use crate::projects::LOAD_PID;

    #[test]
    fn get_latest_minting_report_test() {
        let res = get_flp_own_minting_report(&LOAD_PID).unwrap();
        println!("{res}");
        assert_eq!(res.len(), 43);
    }
}
