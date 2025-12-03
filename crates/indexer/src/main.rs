mod backfill;
mod clickhouse;
mod config;
mod indexer;

use anyhow::Result;
use config::Config;
use indexer::Indexer;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load();
    let clickhouse = clickhouse::Clickhouse::new(&config);
    let indexer = Indexer::new(config, clickhouse);
    indexer.run().await
}
