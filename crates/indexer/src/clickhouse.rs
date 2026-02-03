use anyhow::Result;
use chrono::{DateTime, Utc};
use clickhouse::{Client, Row};
use explorer::BlockStats;
use serde::{Deserialize, Serialize};

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
            "create table if not exists wallet_balances(ts DateTime64(3), ticker String, wallet String, eoa String, amount String, tx_id String) engine=ReplacingMergeTree order by (ticker, wallet, ts)",
            "create table if not exists wallet_delegations(ts DateTime64(3), wallet String, payload String) engine=ReplacingMergeTree order by (wallet, ts)",
            "create table if not exists flp_positions(ts DateTime64(3), ticker String, wallet String, eoa String, project String, factor UInt32, amount String) engine=ReplacingMergeTree order by (project, wallet, ts)",
            "create table if not exists delegation_mappings(ts DateTime64(3), height UInt32, tx_id String, wallet_from String, wallet_to String, factor UInt32) engine=ReplacingMergeTree order by (height, tx_id, wallet_from, wallet_to)",
            "create table if not exists atlas_explorer(ts DateTime64(3), height UInt64, tx_count UInt64, eval_count UInt64, transfer_count UInt64, new_process_count UInt64, new_module_count UInt64, active_users UInt64, active_processes UInt64, tx_count_rolling UInt64, processes_rolling UInt64, modules_rolling UInt64) engine=ReplacingMergeTree order by height",
            "create table if not exists ao_mainnet_explorer(ts DateTime64(3), height UInt64, tx_count UInt64, eval_count UInt64, transfer_count UInt64, new_process_count UInt64, new_module_count UInt64, active_users UInt64, active_processes UInt64, tx_count_rolling UInt64, processes_rolling UInt64, modules_rolling UInt64) engine=ReplacingMergeTree order by height",
            "create table if not exists ao_mainnet_messages(ts DateTime64(3), protocol String, block_height UInt32, block_timestamp UInt64, msg_id String, owner String, recipient String, bundled_in String, data_size String) engine=ReplacingMergeTree order by (protocol, block_height, msg_id)",
            "create table if not exists ao_mainnet_message_tags(ts DateTime64(3), protocol String, block_height UInt32, msg_id String, tag_key String, tag_value String) engine=ReplacingMergeTree order by (tag_key, tag_value, block_height, msg_id)",
            "create table if not exists ao_mainnet_block_state(protocol String, last_complete_height UInt32, last_cursor String, updated_at DateTime64(3)) engine=ReplacingMergeTree order by protocol",
            "create table if not exists ao_token_messages(ts DateTime64(3), source String, block_height UInt32, block_timestamp UInt64, msg_id String, owner String, recipient String, bundled_in String, data_size String) engine=ReplacingMergeTree order by (source, block_height, msg_id)",
            "create table if not exists ao_token_message_tags(ts DateTime64(3), source String, block_height UInt32, msg_id String, tag_key String, tag_value String) engine=ReplacingMergeTree order by (source, tag_key, tag_value, block_height, msg_id)",
            "create table if not exists ao_token_block_state(last_complete_height UInt32, updated_at DateTime64(3)) engine=ReplacingMergeTree order by updated_at",
        ];
        for stmt in stmts {
            self.client.query(stmt).execute().await?;
        }
        let alters = [
            "alter table wallet_balances add column if not exists eoa String after wallet",
            "alter table wallet_balances add column if not exists ar_balance String after amount",
            "alter table flp_positions add column if not exists eoa String after wallet",
            "alter table flp_positions add column if not exists ar_amount String after amount",
            "alter table flp_positions modify column project String",
            "alter table delegation_mappings add column if not exists ts DateTime64(3) default now()",
        ];
        for stmt in alters {
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
    pub async fn insert_delegation_mappings(&self, rows: &[DelegationMappingRow]) -> Result<()> {
        self.insert_rows("delegation_mappings", rows).await
    }
    pub async fn insert_explorer_stats(&self, rows: &[AtlasExplorerRow]) -> Result<()> {
        self.insert_rows("atlas_explorer", rows).await
    }

    pub async fn insert_mainnet_messages(&self, rows: &[MainnetMessageRow]) -> Result<()> {
        self.insert_rows("ao_mainnet_messages", rows).await
    }

    pub async fn insert_mainnet_message_tags(&self, rows: &[MainnetMessageTagRow]) -> Result<()> {
        self.insert_rows("ao_mainnet_message_tags", rows).await
    }

    pub async fn insert_mainnet_block_state(&self, rows: &[MainnetBlockStateRow]) -> Result<()> {
        self.insert_rows("ao_mainnet_block_state", rows).await
    }

    pub async fn insert_ao_token_messages(&self, rows: &[AoTokenMessageRow]) -> Result<()> {
        self.insert_rows("ao_token_messages", rows).await
    }

    pub async fn insert_ao_token_message_tags(&self, rows: &[AoTokenMessageTagRow]) -> Result<()> {
        self.insert_rows("ao_token_message_tags", rows).await
    }

    pub async fn insert_ao_token_block_state(&self, rows: &[AoTokenBlockStateRow]) -> Result<()> {
        self.insert_rows("ao_token_block_state", rows).await
    }

    pub async fn truncate_mainnet_explorer(&self) -> Result<()> {
        self.client
            .query("truncate table if exists ao_mainnet_explorer")
            .execute()
            .await?;
        Ok(())
    }

    pub async fn insert_mainnet_explorer_rows(&self, rows: &[MainnetExplorerRow]) -> Result<()> {
        self.insert_rows("ao_mainnet_explorer", rows).await
    }

    pub async fn latest_mainnet_explorer_row(&self) -> Result<Option<MainnetExplorerRow>> {
        let rows = self
            .client
            .query(
                "select ts, height, tx_count, eval_count, transfer_count, \
                 new_process_count, new_module_count, active_users, active_processes, \
                 tx_count_rolling, processes_rolling, modules_rolling \
                 from ao_mainnet_explorer \
                 order by height desc \
                 limit 1",
            )
            .fetch_all::<MainnetExplorerRow>()
            .await?;
        Ok(rows.into_iter().next())
    }

    pub async fn fetch_mainnet_block_metrics(
        &self,
        after_height: u32,
        limit: u64,
    ) -> Result<Vec<MainnetBlockMetricRow>> {
        let query = "\
            select \
                toDateTime64(max(m.block_timestamp), 3) as ts, \
                max(m.block_timestamp) as ts_unix, \
                m.block_height as height, \
                count() as tx_count, \
                countIf(lowerUTF8(t.tag_key) = 'action' and lowerUTF8(t.tag_value) = 'eval') as eval_count, \
                countIf(lowerUTF8(t.tag_key) = 'action' and lowerUTF8(t.tag_value) = 'transfer') as transfer_count, \
                countIf(lowerUTF8(t.tag_key) = 'type' and lowerUTF8(t.tag_value) = 'process') as new_process_count, \
                countIf(lowerUTF8(t.tag_key) = 'type' and lowerUTF8(t.tag_value) = 'module') as new_module_count, \
                uniqExact(m.owner) as active_users, \
                uniqExactIf(t.tag_value, lowerUTF8(t.tag_key) in ('from-process','process','from-process-id','process-id')) as active_processes \
            from ao_mainnet_messages m \
            left join ao_mainnet_message_tags t \
              on t.protocol = m.protocol and t.block_height = m.block_height and t.msg_id = m.msg_id \
            where m.block_height > ? \
            group by m.block_height \
            order by m.block_height asc \
            limit ?";
        let rows = self
            .client
            .query(query)
            .bind(after_height)
            .bind(limit)
            .fetch_all::<MainnetBlockMetricRow>()
            .await?;
        Ok(rows)
    }

    pub async fn fetch_mainnet_block_state(
        &self,
        protocol: &str,
    ) -> Result<Option<MainnetBlockStateRow>> {
        let rows = self
            .client
            .query(
                "select updated_at, protocol, last_complete_height, last_cursor \
                 from ao_mainnet_block_state \
                 where protocol = ? \
                 order by updated_at desc \
                 limit 1",
            )
            .bind(protocol)
            .fetch_all::<MainnetBlockStateRow>()
            .await?;
        Ok(rows.into_iter().next())
    }

    pub async fn fetch_ao_token_block_state(&self) -> Result<Option<AoTokenBlockStateRow>> {
        let rows = self
            .client
            .query(
                "select last_complete_height, updated_at \
                 from ao_token_block_state \
                 order by updated_at desc \
                 limit 1",
            )
            .fetch_all::<AoTokenBlockStateRow>()
            .await?;
        Ok(rows.into_iter().next())
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

    pub async fn has_delegation_mapping(&self, tx_id: &str) -> Result<bool> {
        let query = "select count() as cnt from delegation_mappings where tx_id = ? limit 1";
        let row = self
            .client
            .query(query)
            .bind(tx_id)
            .fetch_one::<CountRow>()
            .await?;
        Ok(row.cnt > 0)
    }

    pub async fn latest_explorer_stats(&self) -> Result<Option<BlockStats>> {
        let rows = self
            .client
            .query(
                "select ts, height, tx_count, eval_count, transfer_count, new_process_count, new_module_count, active_users, active_processes, tx_count_rolling, processes_rolling, modules_rolling \
                 from atlas_explorer \
                 order by height desc \
                 limit 1",
            )
            .fetch_all::<ExplorerSelectRow>()
            .await?;
        Ok(rows.into_iter().next().map(|row| row.into()))
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
    pub eoa: String,
    pub amount: String,
    pub ar_balance: String,
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
    pub eoa: String,
    pub project: String,
    pub factor: u32,
    pub amount: String,
    pub ar_amount: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct DelegationMappingRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub height: u32,
    pub tx_id: String,
    pub wallet_from: String,
    pub wallet_to: String,
    pub factor: u32,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct AtlasExplorerRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub height: u64,
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

#[derive(Clone, Debug, Row, Serialize)]
pub struct MainnetMessageRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub protocol: String,
    pub block_height: u32,
    pub block_timestamp: u64,
    pub msg_id: String,
    pub owner: String,
    pub recipient: String,
    pub bundled_in: String,
    pub data_size: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct MainnetMessageTagRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub protocol: String,
    pub block_height: u32,
    pub msg_id: String,
    pub tag_key: String,
    pub tag_value: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct AoTokenMessageRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub source: String,
    pub block_height: u32,
    pub block_timestamp: u64,
    pub msg_id: String,
    pub owner: String,
    pub recipient: String,
    pub bundled_in: String,
    pub data_size: String,
}

#[derive(Clone, Debug, Row, Serialize)]
pub struct AoTokenMessageTagRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub source: String,
    pub block_height: u32,
    pub msg_id: String,
    pub tag_key: String,
    pub tag_value: String,
}

#[derive(Clone, Debug, Row, Serialize, Deserialize)]
pub struct AoTokenBlockStateRow {
    pub last_complete_height: u32,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Row, Serialize, Deserialize)]
pub struct MainnetBlockStateRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub updated_at: DateTime<Utc>,
    pub protocol: String,
    pub last_complete_height: u32,
    pub last_cursor: String,
}

#[derive(Clone, Debug, Row, Serialize, Deserialize)]
pub struct MainnetExplorerRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub height: u64,
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

#[derive(Clone, Debug, Row, Serialize, Deserialize)]
pub struct MainnetBlockMetricRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub ts_unix: u64,
    pub height: u32,
    pub tx_count: u64,
    pub eval_count: u64,
    pub transfer_count: u64,
    pub new_process_count: u64,
    pub new_module_count: u64,
    pub active_users: u64,
    pub active_processes: u64,
}

impl AtlasExplorerRow {
    pub fn from_block_stats(stats: &BlockStats) -> Option<Self> {
        let ts =
            DateTime::<Utc>::from_timestamp_millis((stats.timestamp as i64).saturating_mul(1000))?;
        Some(Self {
            ts,
            height: stats.height,
            tx_count: stats.tx_count,
            eval_count: stats.eval_count,
            transfer_count: stats.transfer_count,
            new_process_count: stats.new_process_count,
            new_module_count: stats.new_module_count,
            active_users: stats.active_users,
            active_processes: stats.active_processes,
            tx_count_rolling: stats.tx_count_rolling,
            processes_rolling: stats.processes_rolling,
            modules_rolling: stats.modules_rolling,
        })
    }
}

#[derive(Debug, Row, Serialize, serde::Deserialize)]
struct ExplorerSelectRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    height: u64,
    tx_count: u64,
    eval_count: u64,
    transfer_count: u64,
    new_process_count: u64,
    new_module_count: u64,
    active_users: u64,
    active_processes: u64,
    tx_count_rolling: u64,
    processes_rolling: u64,
    modules_rolling: u64,
}

impl From<ExplorerSelectRow> for BlockStats {
    fn from(row: ExplorerSelectRow) -> Self {
        BlockStats {
            height: row.height,
            timestamp: (row.ts.timestamp_millis() / 1000) as u64,
            tx_count: row.tx_count,
            eval_count: row.eval_count,
            transfer_count: row.transfer_count,
            new_process_count: row.new_process_count,
            new_module_count: row.new_module_count,
            active_users: row.active_users,
            active_processes: row.active_processes,
            tx_count_rolling: row.tx_count_rolling,
            processes_rolling: row.processes_rolling,
            modules_rolling: row.modules_rolling,
        }
    }
}
#[derive(Debug, Row, Serialize, serde::Deserialize)]
struct CountRow {
    pub cnt: u64,
}
