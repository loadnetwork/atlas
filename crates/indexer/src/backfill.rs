use anyhow::Result;
use chrono::Utc;
use common::delegation::{
    DELEGATION_PID_START_HEIGHT, DelegationMappingMeta, DelegationMappingsPage,
    get_delegation_mappings,
};
use flp::csv_parser::parse_delegation_mappings_res;
use tokio::time::{Duration, sleep};

use crate::clickhouse::{Clickhouse, DelegationMappingRow};

const TARGET_HEIGHT: u32 = 1_807_500;
const PAGE_SIZE: u32 = 100;
const DELAY_SECS: u64 = 300;

pub async fn run(clickhouse: Clickhouse) -> Result<()> {
    println!("delegation backfill starting");
    let mut after: Option<String> = None;
    loop {
        let page = fetch_page(after.as_deref())?;
        println!(
            "backfill fetched {} mappings (has_next_page={}, cursor={:?})",
            page.mappings.len(),
            page.has_next_page,
            page.end_cursor
        );
        if page.mappings.is_empty() {
            println!("backfill empty page, stopping");
            break;
        }
        for meta in page.mappings.iter() {
            if meta.height < DELEGATION_PID_START_HEIGHT || meta.height > TARGET_HEIGHT {
                continue;
            }
            if clickhouse.has_delegation_mapping(&meta.tx_id).await? {
                continue;
            }
            println!(
                "backfill indexing delegation mapping tx {} height {}",
                meta.tx_id, meta.height
            );
            if let Err(err) = process_tx(&clickhouse, meta).await {
                eprintln!("backfill failed to index {}: {err:?}", meta.tx_id);
            }
            sleep(Duration::from_secs(DELAY_SECS)).await;
        }
        if !page.has_next_page {
            println!("backfill no next page, stopping");
            break;
        }
        after = page.end_cursor.clone();
        if after.is_none() {
            println!("backfill cursor missing, stopping");
            break;
        }
    }
    println!("delegation backfill finished");
    Ok(())
}

fn fetch_page(after: Option<&str>) -> Result<DelegationMappingsPage> {
    get_delegation_mappings(Some(PAGE_SIZE), after)
}

async fn process_tx(clickhouse: &Clickhouse, meta: &DelegationMappingMeta) -> Result<()> {
    let csv_rows = parse_delegation_mappings_res(&meta.tx_id)?;
    let ts = Utc::now();
    let rows: Vec<DelegationMappingRow> = csv_rows
        .into_iter()
        .map(|row| DelegationMappingRow {
            ts,
            height: meta.height,
            tx_id: meta.tx_id.clone(),
            wallet_from: row.wallet_from,
            wallet_to: row.wallet_to,
            factor: row.factor,
        })
        .collect();
    clickhouse.insert_delegation_mappings(&rows).await?;
    Ok(())
}
