use anyhow::Result;
use chrono::Utc;
use common::delegation::{
    get_delegation_mappings, DelegationMappingMeta, DelegationMappingsPage,
    DELEGATION_PID_START_HEIGHT,
};
use flp::csv_parser::parse_delegation_mappings_res;
use indexer::clickhouse::{Clickhouse, DelegationMappingRow};
use indexer::config::Config;
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load();
    let clickhouse = Clickhouse::new(&config);
    clickhouse.ensure().await?;

    let target: u32 = 1_807_500;
    let mut after: Option<String> = None;
    loop {
        let page = fetch_page(after.as_deref())?;
        println!(
            "fetched {} mappings (has_next_page={}, cursor={:?})",
            page.mappings.len(),
            page.has_next_page,
            page.end_cursor
        );
        if page.mappings.is_empty() {
            println!("empty page, stopping");
            break;
        }
        for meta in page.mappings.iter() {
            if meta.height < DELEGATION_PID_START_HEIGHT {
                continue;
            }
            if meta.height > target {
                continue;
            }
            println!("indexing delegation mapping tx {} height {}", meta.tx_id, meta.height);
            if let Err(err) = process_tx(&clickhouse, meta).await {
                eprintln!("failed to index {}: {err:?}", meta.tx_id);
            }
            sleep(Duration::from_secs(300)).await;
        }
        if !page.has_next_page {
            println!("no next page, stopping");
            break;
        }
        after = page.end_cursor.clone();
        if after.is_none() {
            println!("cursor missing, stopping");
            break;
        }
    }

    Ok(())
}

fn fetch_page(after: Option<&str>) -> Result<DelegationMappingsPage> {
    get_delegation_mappings(Some(100), after)
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
