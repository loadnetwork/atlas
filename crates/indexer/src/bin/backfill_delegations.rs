use anyhow::Result;
use indexer::{backfill, clickhouse::Clickhouse, config::Config};

#[tokio::main]
async fn main() -> Result<()> {
    // let config = Config::load();
    // let clickhouse = Clickhouse::new(&config);
    // clickhouse.ensure().await?;
    // backfill::run(clickhouse).await
    Ok(())
}
