use crate::constants::{AO_AUTHORITY, AO_TOKEN_PROCESS, ARWEAVE_GATEWAY};
use anyhow::{Error, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy)]
pub enum AoTokenQuery {
    Transfer,
    Process,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AoTokenMessageMeta {
    pub msg_id: String,
    pub owner: String,
    pub recipient: String,
    pub block_height: u32,
    pub block_timestamp: u64,
    pub bundled_in: String,
    pub data_size: String,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct AoTokenMessagesPage {
    pub mappings: Vec<AoTokenMessageMeta>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

pub fn scan_arweave_block_for_ao_token_msgs(
    query: AoTokenQuery,
    blockheight: u32,
    after: Option<&str>,
) -> Result<AoTokenMessagesPage, Error> {
    let (filter_clause, query_label) = match query {
        AoTokenQuery::Transfer => (
            format!(
                "owners: [\"{AO_AUTHORITY}\"]\n    recipients: [\"{AO_TOKEN_PROCESS}\"]\n    tags: [{{ name: \"Action\", values: [\"Transfer\"] }}]"
            ),
            "aoTokenTransfers",
        ),
        AoTokenQuery::Process => (
            format!(
                "owners: [\"{AO_AUTHORITY}\"]\n    tags: [{{ name: \"From-Process\", values: [\"{AO_TOKEN_PROCESS}\"] }}]"
            ),
            "aoTokenProcessMsgs",
        ),
    };

    let template = r#"
query $querylabel {
  transactions(
    first: 100
    sort: HEIGHT_ASC
    $afterclause
    $filterclause
    block: { min: $blockheight, max: $blockheight }
  ) {
    edges {
      cursor
      node {
        id
        owner {
          address
        }
        recipient
        tags {
          name
          value
        }
        block {
          id
          height
          timestamp
        }
        bundledIn {
          id
        }
        data {
          size
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
    let gql_query = template
        .replace("$querylabel", query_label)
        .replace("$afterclause", &after_clause)
        .replace("$filterclause", &filter_clause)
        .replace("$blockheight", &blockheight.to_string());

    let body = json!({
        "query": gql_query,
        "variables": {}
    });

    let req = ureq::post(format!("{ARWEAVE_GATEWAY}/graphql"))
        .send_json(body)?
        .body_mut()
        .read_to_string()?;
    let res: Value = serde_json::from_str(&req)?;

    let txs = res
        .get("data")
        .and_then(|v| v.get("transactions"))
        .ok_or(anyhow!(
            "error: no transactions object found for the ao token query"
        ))?;
    let has_next_page = txs
        .get("pageInfo")
        .and_then(|v| v.get("hasNextPage"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let edges = txs.get("edges").and_then(|v| v.as_array());
    let mut out = Vec::new();
    let mut last_cursor = None;
    if let Some(edges) = edges {
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
            let block_height = node
                .get("block")
                .and_then(|v| v.get("height"))
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .unwrap_or(0);
            let block_timestamp = node
                .get("block")
                .and_then(|v| v.get("timestamp"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tags = node
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| {
                            Some(Tag {
                                key: t.get("name")?.as_str()?.to_string(),
                                value: t.get("value")?.as_str()?.to_string(),
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if matches!(query, AoTokenQuery::Transfer) && !has_action_transfer(&tags) {
                continue;
            }

            let owner = node
                .get("owner")
                .and_then(|o| o.get("address"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let recipient = node
                .get("recipient")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let data_size = node
                .get("data")
                .and_then(|v| v.get("size"))
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();

            let bundled_in: String = node
                .get("bundledIn")
                .and_then(|v| v.get("id"))
                .and_then(|s| s.as_str())
                .unwrap_or_default()
                .to_string();

            out.push(AoTokenMessageMeta {
                msg_id: id.to_string(),
                block_height,
                block_timestamp,
                owner,
                recipient,
                tags,
                data_size,
                bundled_in,
            });
        }
    }

    Ok(AoTokenMessagesPage {
        mappings: out,
        has_next_page,
        end_cursor: last_cursor,
    })
}

fn has_action_transfer(tags: &[Tag]) -> bool {
    tags.iter().any(|tag| {
        tag.key.eq_ignore_ascii_case("action") && tag.value.eq_ignore_ascii_case("transfer")
    })
}
