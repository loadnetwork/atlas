/// common utils for retrieving, filtering and sorting
/// ao mainnet network data (ao.N.1 messages) extracted from
/// Arweave blocks using GQL gateways.
///
/// the mainnet have 2 type of tags for messages, in Atlas,
/// we label them as type A and type B:
///
/// - type A follows lower-case tags key format
/// - type B follows Header-Case tags key format
/// - type A start blockheight: 1_594_020 -- Jan 22 2025
/// - type B start blockheight: 1_616_999 --  Feb 25 2025
use crate::constants::{ARWEAVE_GATEWAY, DATA_PROTOCOL_A_START, DATA_PROTOCOL_B_START};
use anyhow::{Error, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Serialize, Deserialize)]
pub enum DataProtocol {
    A,
    B,
}

impl DataProtocol {
    pub fn tags(&self) -> String {
        match self {
            Self::A => r#"{ name: "variant", values: ["ao.N.1"] }, { name: "data-protocol", values: ["ao"] }"#.to_string(),
            Self::B => r#"{ name: "Variant", values: ["ao.N.1"] }, { name: "Data-Protocol", values: ["ao"] }"#.to_string(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Tag {
    pub key: String,
    pub value: String,
}

impl Tag {
    pub fn from_kv(key: &str, value: &str) -> Self {
        Self {
            key: key.to_string(),
            value: value.to_string(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MainnetBlockMessagesMeta {
    pub msg_id: String,
    pub owner: String,
    pub recipient: String,
    pub block_height: u32,
    pub block_timestamp: u64,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MainnetBlockMessagesPage {
    pub mappings: Vec<MainnetBlockMessagesMeta>,
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

pub fn scan_arweave_block_for_msgs(
    data_protocol: DataProtocol,
    blockheight: u32,
    after: Option<&str>,
) -> Result<MainnetBlockMessagesPage, Error> {
    let query_tags = data_protocol.tags();
    let template = r#"

query aoMainnet {
    transactions(
      sort: HEIGHT_ASC
      first: 100
      $afterclause
        tags: [$dataprotocol_tags]
        block: { min: $blockheight, max: $blockheight }
    ) {
        edges {
            node {
                id
                recipient
                tags {
                    name
                    value
                }
                owner {
                    address
                }
                block {
                    height
                    timestamp
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
        .replace("$dataprotocol_tags", &query_tags)
        .replace("$afterclause", &after_clause)
        .replace("$blockheight", &blockheight.to_string());

    let body = json!({
        "query": query,
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
            "error: no transactions object found for the ao mainnet blocks query"
        ))?;
    let has_next_page = txs
        .get("pageInfo")
        .and_then(|v| v.get("hasNextPage"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let edges = txs.get("edges").and_then(|v| v.as_array()).ok_or(anyhow!(
        "error: no ao message edges found for the ao mainnet blocks query"
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
            .map(|v| v as u64)
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

        out.push(MainnetBlockMessagesMeta {
            msg_id: id.to_string(),
            block_height,
            block_timestamp,
            owner,
            recipient,
            tags,
        });
    }

    if out.is_empty() {
        return Err(anyhow!("error: no ao message id found for the given query"));
    }
    Ok(MainnetBlockMessagesPage {
        mappings: out,
        has_next_page,
        end_cursor: last_cursor,
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        constants::{DATA_PROTOCOL_A_START, DATA_PROTOCOL_B_START},
        mainnet::{DataProtocol, scan_arweave_block_for_msgs},
    };

    #[test]
    fn scan_protocol_a_genesis_test() {
        let messages =
            scan_arweave_block_for_msgs(DataProtocol::A, DATA_PROTOCOL_A_START, None).unwrap();
        println!("{:?}", messages);
        assert_eq!(
            messages.mappings[0].msg_id,
            "kfwvyN59sihMeSFjBP44ujI_as4ZEQWERrS83ordEkY"
        );
        assert!(!messages.has_next_page);
    }

    #[test]
    fn scan_protocol_b_genesis_test() {
        let messages =
            scan_arweave_block_for_msgs(DataProtocol::B, DATA_PROTOCOL_B_START, None).unwrap();
        println!("{:?}", messages);
        assert_eq!(
            messages.mappings[0].msg_id,
            "FY3NP7-edCq3RIE0aiSxbme2n428XdiTvp3gJKbMISQ"
        );
        assert!(!messages.has_next_page);
    }

    #[test]
    // simulates an messages-empty block
    fn scan_protocol_a_pre_genesis_test() {
        let err = scan_arweave_block_for_msgs(DataProtocol::A, DATA_PROTOCOL_A_START - 1, None);
        assert!(err.is_err());
    }

    #[test]
    fn recipient_test() {
        let messages =
            scan_arweave_block_for_msgs(DataProtocol::B, 1630347, None).unwrap();
        println!("{:?}", messages);
        assert_eq!(
            messages.mappings[0].recipient,
            "nVOgez-AT87nS_42B9hwlz982BjH0YXp_RW3ezBHoew"
        );
        assert!(!messages.has_next_page);
    }
}
