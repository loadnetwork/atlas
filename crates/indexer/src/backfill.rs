use anyhow::Result;
use chrono::Utc;
use common::delegation::{
    DELEGATION_PID_START_HEIGHT, DelegationMappingMeta, DelegationMappingsPage,
    get_delegation_mappings,
};
use tokio::time::{Duration, sleep};

use crate::clickhouse::{Clickhouse, DelegationMappingRow};

use common::mainnet::{DataProtocol, MainnetBlockMessagesMeta};

use crate::clickhouse::{MainnetMessageRow, MainnetMessageTagRow};
use crate::indexer::{
    fetch_mainnet_page, fetch_network_height, is_empty_block_error, protocol_label,
};

const ARWEAVE_TIP_SAFE_GAP: u64 = 3;

// const TARGET_HEIGHT: u32 = 1_807_500; // thats where the forward indexer starts
// const PAGE_SIZE: u32 = 100;
// const DELAY_SECS: u64 = 600;

// pub async fn run(clickhouse: Clickhouse) -> Result<()> {
//     println!("delegation backfill starting");
//     let mut after: Option<String> = None;
//     loop {
//         let page = fetch_page(after.as_deref())?;
//         println!(
//             "backfill fetched {} mappings (has_next_page={}, cursor={:?})",
//             page.mappings.len(),
//             page.has_next_page,
//             page.end_cursor
//         );
//         if page.mappings.is_empty() {
//             println!("backfill empty page, stopping");
//             break;
//         }
//         for meta in page.mappings.iter() {
//             if meta.height < DELEGATION_PID_START_HEIGHT {
//                 println!(
//                     "backfill skipping tx {} height {} (< start)",
//                     meta.tx_id, meta.height
//                 );
//                 continue;
//             }
//             if meta.height > TARGET_HEIGHT {
//                 println!(
//                     "backfill skipping tx {} height {} (> target)",
//                     meta.tx_id, meta.height
//                 );
//                 continue;
//             }
//             if clickhouse.has_delegation_mapping(&meta.tx_id).await? {
//                 println!(
//                     "backfill skip tx {} height {} already stored",
//                     meta.tx_id, meta.height
//                 );
//                 continue;
//             }
//             println!(
//                 "backfill indexing delegation mapping tx {} height {}",
//                 meta.tx_id, meta.height
//             );
//             match process_tx(&clickhouse, meta).await {
//                 Ok(count) => println!("backfill stored {} prefs for {}", count, meta.tx_id),
//                 Err(err) => {
//                     eprintln!("backfill failed to index {}: {err:?}", meta.tx_id);
//                     continue;
//                 }
//             }
//             sleep(Duration::from_secs(DELAY_SECS)).await;
//         }
//         if !page.has_next_page {
//             println!("backfill no next page, stopping");
//             break;
//         }
//         after = page.end_cursor.clone();
//         if after.is_none() {
//             println!("backfill cursor missing, stopping");
//             break;
//         }
//     }
//     println!("delegation backfill finished");
//     Ok(())
// }

// fn fetch_page(after: Option<&str>) -> Result<DelegationMappingsPage> {
//     get_delegation_mappings(Some(PAGE_SIZE), after)
// }

// async fn process_tx(clickhouse: &Clickhouse, meta: &DelegationMappingMeta) -> Result<usize> {
//     let csv_rows = parse_delegation_mappings_res(&meta.tx_id)?;
//     let ts = Utc::now();
//     let rows: Vec<DelegationMappingRow> = csv_rows
//         .into_iter()
//         .map(|row| DelegationMappingRow {
//             ts,
//             height: meta.height,
//             tx_id: meta.tx_id.clone(),
//             wallet_from: row.wallet_from,
//             wallet_to: row.wallet_to,
//             factor: row.factor,
//         })
//         .collect();
//     let count = rows.len();
//     clickhouse.insert_delegation_mappings(&rows).await?;
//     Ok(count)
// }

// ao.N.1 blocks gap filler

pub async fn run_mainnet_gap_worker(
    clickhouse: Clickhouse,
    protocol: DataProtocol,
    start: u32,
) -> Result<()> {
    let protocol_name = protocol_label(protocol).to_string();
    let mut height = start;
    let mut cursor = None;
    loop {
        let tip = fetch_network_height().await.unwrap_or(height as u64);
        if height as u64 + ARWEAVE_TIP_SAFE_GAP > tip {
            break;
        }
        let page = match fetch_mainnet_page(protocol, height, cursor.clone()).await {
            Ok(page) => page,
            Err(err) => {
                if is_empty_block_error(&err) {
                    cursor = None;
                    println!("gap protocol {} height {} empty", protocol_name, height);
                    height = height.saturating_add(1);
                } else {
                    eprintln!(
                        "gap fetch error protocol={} height={} err={err:?}",
                        protocol_name, height
                    );
                    sleep(Duration::from_secs(1)).await;
                }
                continue;
            }
        };
        let ts = Utc::now();
        let mut message_rows = Vec::with_capacity(page.mappings.len());
        let mut tag_rows = Vec::new();
        for meta in page.mappings {
            let MainnetBlockMessagesMeta {
                msg_id,
                owner,
                recipient,
                block_height,
                block_timestamp,
                bundled_in,
                data_size,
                tags,
            } = meta;
            let msg_id_for_tags = msg_id.clone();
            message_rows.push(MainnetMessageRow {
                ts,
                protocol: protocol_name.clone(),
                block_height,
                block_timestamp,
                msg_id,
                owner,
                recipient,
                bundled_in,
                data_size,
            });
            for tag in tags {
                tag_rows.push(MainnetMessageTagRow {
                    ts,
                    protocol: protocol_name.clone(),
                    block_height,
                    msg_id: msg_id_for_tags.clone(),
                    tag_key: tag.key,
                    tag_value: tag.value,
                });
            }
        }
        clickhouse.insert_mainnet_messages(&message_rows).await?;
        clickhouse.insert_mainnet_message_tags(&tag_rows).await?;
        cursor = if page.has_next_page {
            page.end_cursor.clone()
        } else {
            None
        };
        if cursor.is_none() {
            height = height.saturating_add(1);
        }
    }
    Ok(())
}
