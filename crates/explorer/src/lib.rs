use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
pub mod update_stats_gap;

const ENDPOINT: &str = "https://permagate.io/graphql";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AoTx {
    pub id: String,
    pub block_height: u64,
    pub block_timestamp: i64,
    pub owner: String,
    pub tx_type: Option<String>,
    pub action: Option<String>,
    pub process: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AoPage {
    pub txs: Vec<AoTx>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockStats {
    pub height: u64,
    pub timestamp: u64,
    pub tx_count: u64,
    pub eval_count: u64,
    pub transfer_count: u64,
    pub new_process_count: u64,
    pub new_module_count: u64,
    pub active_users: u64,
    pub active_processes: u64,
    pub tx_count_rolling: u64,
    pub processes_rolling: u64,
    pub modules_rolling: u64,
}

pub fn fetch_ao_page(height: u32) -> Result<AoPage> {
    fetch_ao_page_with_cursor(height, None)
}

fn fetch_ao_page_with_cursor(height: u32, cursor: Option<&str>) -> Result<AoPage> {
    let template = r#"
query GetAoTxs {
  transactions(
    first: 100
    sort: HEIGHT_DESC,
    block: {min:  $blockid, max: $blockid},
$cursor_clause
tags: [
      { name: "Data-Protocol", values: ["ao"] }
    ]
  ) {
    edges {
      cursor
      node {
        id
        owner { address }
        block { height timestamp }
        tags { name value }
      }
    }
    pageInfo {
      hasNextPage
    }
  }
}
"#;
    let cursor_clause = cursor
        .map(|c| format!("    after: \"{}\",\n", c))
        .unwrap_or_default();
    let query = template
        .replace("$blockid", &height.to_string())
        .replace("$cursor_clause", &cursor_clause);
    let body = json!({
        "query": query,
        "variables": {}
    });
    let mut res = ureq::post(ENDPOINT).send_json(body)?;
    let res = res.body_mut().read_to_string()?;
    let res: GraphResponse = serde_json::from_str(&res)?;
    let data = res.data.ok_or_else(|| anyhow!("missing data"))?;
    let page = data.transactions;
    let mut end_cursor = None;
    let txs = page
        .edges
        .into_iter()
        .map(|edge| {
            end_cursor = Some(edge.cursor);
            AoTx::from_node(edge.node)
        })
        .collect();
    Ok(AoPage {
        txs,
        cursor: end_cursor,
        has_more: page.page_info.has_next_page,
    })
}

pub fn fetch_full_block(height: u32) -> Result<Vec<AoTx>> {
    let mut cursor = None;
    let mut all = Vec::new();
    loop {
        let page = fetch_ao_page_with_cursor(height, cursor.as_deref())?;
        let has_more = page.has_more;
        cursor = page.cursor.clone();
        all.extend(page.txs);
        if !has_more || cursor.is_none() {
            break;
        }
    }
    Ok(all)
}

pub fn aggregate_block_full(height: u32) -> Result<Vec<BlockStats>> {
    let txs = fetch_full_block(height)?;
    Ok(aggregate_block(&txs))
}

pub fn aggregate_block(txs: &[AoTx]) -> Vec<BlockStats> {
    let mut grouped: BTreeMap<u64, Vec<&AoTx>> = BTreeMap::new();
    for tx in txs {
        grouped.entry(tx.block_height).or_default().push(tx);
    }
    let mut out = Vec::new();
    let mut tx_roll = 0;
    let mut proc_roll = 0;
    let mut mod_roll = 0;
    for (height, block) in grouped {
        let ts = block
            .get(0)
            .and_then(|t| Some(t.block_timestamp.to_string()))
            .and_then(|timestamp| Some(timestamp.parse::<u64>().unwrap_or_default()));
        let tx_count = block.len() as u64;
        let eval_count = block
            .iter()
            .filter(|t| t.action.as_deref() == Some("Eval"))
            .count() as u64;
        let transfer_count = block
            .iter()
            .filter(|t| t.action.as_deref() == Some("Transfer"))
            .count() as u64;
        let new_process_count = block
            .iter()
            .filter(|t| t.tx_type.as_deref() == Some("Process"))
            .count() as u64;
        let new_module_count = block
            .iter()
            .filter(|t| t.tx_type.as_deref() == Some("Module"))
            .count() as u64;
        let mut users = HashSet::new();
        let mut processes = HashSet::new();
        for tx in &block {
            users.insert(&tx.owner);
            if let Some(p) = &tx.process {
                processes.insert(p);
            }
        }
        tx_roll += tx_count;
        proc_roll += new_process_count;
        mod_roll += new_module_count;
        out.push(BlockStats {
            height: height,
            timestamp: ts.unwrap_or(0),
            tx_count,
            eval_count,
            transfer_count,
            new_process_count,
            new_module_count,
            active_users: users.len() as u64,
            active_processes: processes.len() as u64,
            tx_count_rolling: tx_roll,
            processes_rolling: proc_roll,
            modules_rolling: mod_roll,
        });
    }
    out
}

impl AoTx {
    fn from_node(node: GraphNode) -> Self {
        let mut tx_type = None;
        let mut action = None;
        let mut process = None;
        for tag in node.tags {
            match tag.name.as_str() {
                "Type" => tx_type = Some(tag.value),
                "Action" => action = Some(tag.value),
                "From-Process" => process = Some(tag.value),
                "Process" => {
                    if process.is_none() {
                        process = Some(tag.value);
                    }
                }
                _ => {}
            }
        }
        AoTx {
            id: node.id,
            block_height: node.block.height,
            block_timestamp: node.block.timestamp.unwrap_or(0),
            owner: node.owner.address,
            tx_type,
            action,
            process,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct GraphResponse {
    data: Option<GraphData>,
}

#[derive(Serialize, Deserialize)]
struct GraphData {
    transactions: GraphTransactions,
}

#[derive(Serialize, Deserialize)]
struct GraphTransactions {
    edges: Vec<Edge>,
    #[serde(rename = "pageInfo")]
    page_info: PageInfo,
}

#[derive(Serialize, Deserialize)]
struct Edge {
    cursor: String,
    node: GraphNode,
}

#[derive(Serialize, Deserialize)]
struct GraphNode {
    id: String,
    owner: Owner,
    block: Block,
    tags: Vec<Tag>,
}

#[derive(Serialize, Deserialize)]
struct Owner {
    address: String,
}

#[derive(Serialize, Deserialize)]
struct Block {
    height: u64,
    timestamp: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct Tag {
    name: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
struct PageInfo {
    #[serde(rename = "hasNextPage")]
    has_next_page: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_page_empty() {
        let block_number = 1_810_247_u32;
        let page = fetch_ao_page(block_number).unwrap();
        assert!(page.txs.is_empty())
    }
    #[test]
    fn fetch_page_not_empty() {
        let block_number = 1_810_252_u32;
        let page = fetch_ao_page(block_number).unwrap();
        assert!(!page.txs.is_empty())
    }

    #[test]
    fn aggregate_block_1810252() {
        let block_number = 1_810_252_u32;
        let aggregation = aggregate_block_full(block_number).unwrap();
        println!("aggregating block #{block_number} \n {:#?}", aggregation);
    }
}
