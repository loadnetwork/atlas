use anyhow::Result;
use chrono::{DateTime, Utc};
use clickhouse::{Client, Row};
use serde::Serialize;

use crate::config::Config;

#[derive(Clone)]
pub struct Clickhouse {
    client: Client,
    admin: Client,
    database: String,
}

impl Clickhouse {
    pub fn new(config: &Config) -> Self {
        let admin = Client::default()
            .with_url(&config.clickhouse_url)
            .with_user(&config.clickhouse_user)
            .with_password(&config.clickhouse_password);
        let client = admin.clone().with_database(&config.clickhouse_database);
        Clickhouse {
            client,
            admin,
            database: config.clickhouse_database.clone(),
        }
    }

    pub async fn ensure(&self) -> Result<()> {
        let create_db = format!("create database if not exists {}", self.database);
        self.admin.query(&create_db).execute().await?;
        let stmts = [
            "create table if not exists oracle_snapshots(ts DateTime64(3), ticker String, tx_id String) engine=MergeTree order by (ticker, ts)",
            "create table if not exists wallet_balances(ts DateTime64(3), ticker String, wallet String, amount String, tx_id String) engine=ReplacingMergeTree order by (ticker, wallet, ts)",
            "create table if not exists wallet_delegations(ts DateTime64(3), wallet String, payload String) engine=ReplacingMergeTree order by (wallet, ts)",
            "create table if not exists flp_positions(ts DateTime64(3), ticker String, wallet String, project String, factor UInt32, amount String) engine=ReplacingMergeTree order by (project, wallet, ts)",
        ];
        for stmt in stmts {
            self.client.query(stmt).execute().await?;
        }
        Ok(())
    }

    pub async fn insert_oracles(&self, rows: &[OracleSnapshotRow]) -> Result<()> {
        self.insert_rows("oracle_snapshots", rows).await
    }

    pub async fn insert_balances(&self, rows: &[WalletBalanceRow]) -> Result<()> {
        self.insert_rows("wallet_balances", rows).await
    }

    pub async fn insert_delegations(&self, rows: &[WalletDelegationRow]) -> Result<()> {
        self.insert_rows("wallet_delegations", rows).await
    }

    pub async fn insert_positions(&self, rows: &[FlpPositionRow]) -> Result<()> {
        self.insert_rows("flp_positions", rows).await
    }

    pub async fn has_oracle(&self, ticker: &str, tx_id: &str) -> Result<bool> {
        let query = format!(
            "select count() as cnt from oracle_snapshots where ticker = ? and tx_id = ? limit 1"
        );
        let row = self
            .client
            .query(&query)
            .bind(ticker)
            .bind(tx_id)
            .fetch_one::<CountRow>()
            .await?;
        Ok(row.cnt > 0)
    }

    async fn insert_rows<T>(&self, table: &str, rows: &[T]) -> Result<()>
    where
        T: Row + Serialize,
    {
        if rows.is_empty() {
            return Ok(());
        }
        let mut insert = self.client.insert(table)?;
        for row in rows {
            insert.write(row).await?;
        }
        insert.end().await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct OracleSnapshotRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub ticker: String,
    pub tx_id: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct WalletBalanceRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub ticker: String,
    pub wallet: String,
    pub amount: String,
    pub tx_id: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct WalletDelegationRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub wallet: String,
    pub payload: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct FlpPositionRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub ticker: String,
    pub wallet: String,
    pub project: String,
    pub factor: u32,
    pub amount: String,
}
#[derive(Debug, Row, Serialize, serde::Deserialize)]
struct CountRow {
    pub cnt: u64,
}
