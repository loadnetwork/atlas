use anyhow::{Error, anyhow};
use chrono::{DateTime, Utc};
use clickhouse::Row;
use common::env::get_env_var;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct AtlasIndexerClient {
    client: clickhouse::Client,
}

impl AtlasIndexerClient {
    pub async fn new() -> Result<Self, Error> {
        let url = get_env_var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
        let user = get_env_var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
        let password = get_env_var("CLICKHOUSE_PASSWORD").unwrap_or_default();
        let database =
            get_env_var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "atlas_oracles".into());
        let admin = clickhouse::Client::default()
            .with_url(&url)
            .with_user(&user)
            .with_password(&password);
        let client = admin.clone().with_database(&database);
        ensure_schema(&admin, &client, &database).await?;
        Ok(Self { client })
    }

    pub async fn latest_project_snapshot(&self, project: &str) -> Result<ProjectSnapshot, Error> {
        let query = "\
            with latest as (\
                select ticker, max(ts) as ts \
                from flp_positions \
                where project = ? \
                group by ticker\
            ) \
            select p.ts, p.ticker, p.wallet, p.eoa, toString(p.project) as project, p.factor, p.amount, p.ar_amount \
            from flp_positions p \
            inner join latest l on p.ticker = l.ticker and p.ts = l.ts \
            where p.project = ? \
            order by p.ticker, p.amount desc";
        let rows = self
            .client
            .query(query)
            .bind(project)
            .bind(project)
            .fetch_all::<FlpPositionRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no delegations found for project {project}"));
        }
        let ts = rows.iter().map(|row| row.ts).max().unwrap();
        let totals = aggregate_totals(&rows);
        let delegators = rows
            .into_iter()
            .map(|row| Delegator {
                wallet: row.wallet,
                eoa: row.eoa,
                ticker: row.ticker,
                factor: row.factor,
                amount: row.amount,
                ar_amount: row.ar_amount,
            })
            .collect();
        Ok(ProjectSnapshot {
            project: project.to_string(),
            ts,
            totals,
            delegators,
        })
    }

    pub async fn wallet_identity_history(&self, wallet: &str) -> Result<Vec<IdentityLink>, Error> {
        let rows = self
            .client
            .query(
                "select wallet, eoa, ts \
                 from wallet_balances \
                 where wallet = ? \
                 order by ts desc",
            )
            .bind(wallet)
            .fetch_all::<IdentityRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn eoa_identity_history(&self, eoa: &str) -> Result<Vec<IdentityLink>, Error> {
        let rows = self
            .client
            .query(
                "select wallet, eoa, ts \
                 from wallet_balances \
                 where eoa = ? \
                 order by ts desc",
            )
            .bind(eoa)
            .fetch_all::<IdentityRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn oracle_snapshot_feed(
        &self,
        ticker: &str,
        limit: u64,
    ) -> Result<Vec<OracleSnapshot>, Error> {
        let rows = self
            .client
            .query(
                "select o.ts, o.ticker, o.tx_id, sum(toFloat64(p.amount)) as total, uniqExact(p.wallet) as delegators \
                 from oracle_snapshots o \
                 left join flp_positions p \
                   on p.ticker = o.ticker and p.ts = o.ts \
                 where o.ticker = ? \
                 group by o.ts, o.ticker, o.tx_id \
                 order by o.ts desc \
                 limit ?",
            )
            .bind(ticker)
            .bind(limit)
            .fetch_all::<OracleSnapshot>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no oracle snapshots found for ticker {ticker}"));
        }
        Ok(rows)
    }

    pub async fn wallet_delegation_mappings(
        &self,
        wallet: &str,
    ) -> Result<Vec<DelegationMappingHistory>, Error> {
        let rows = self
            .client
            .query(
                "select ts, height, tx_id, wallet_from, wallet_to, factor \
                 from delegation_mappings \
                 where wallet_from = ? \
                 order by height desc",
            )
            .bind(wallet)
            .fetch_all::<DelegationMappingRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no delegation mappings found for wallet {wallet}"));
        }
        let mut map = BTreeMap::new();
        for row in rows {
            let key = (row.height, row.tx_id.clone());
            let entry = map.entry(key).or_insert_with(|| DelegationMappingHistory {
                ts: row.ts,
                height: row.height,
                tx_id: row.tx_id.clone(),
                wallet: row.wallet_from.clone(),
                preferences: Vec::new(),
            });
            entry.preferences.push(DelegationPreference {
                wallet_to: row.wallet_to,
                factor: row.factor,
            });
        }
        let mut out: Vec<_> = map.into_values().collect();
        out.sort_by(|a, b| b.height.cmp(&a.height));
        Ok(out)
    }

    pub async fn latest_delegation_heights(
        &self,
        limit: u64,
    ) -> Result<Vec<DelegationHeight>, Error> {
        let rows = self
            .client
            .query(
                "select height, tx_id \
                 from delegation_mappings \
                 group by height, tx_id \
                 order by height desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<DelegationHeightRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no delegation mappings indexed yet"));
        }
        Ok(rows
            .into_iter()
            .map(|row| DelegationHeight {
                height: row.height,
                tx_id: row.tx_id,
            })
            .collect())
    }

    pub async fn multi_project_delegators(&self, limit: u64) -> Result<Vec<MultiDelegator>, Error> {
        let rows = self
            .client
            .query(
                "select wallet, any(eoa) as eoa, countDistinct(project) as project_count, \
                 groupUniqArray(project) as projects \
                 from flp_positions \
                 group by wallet \
                 having project_count >= 2 \
                 order by project_count desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<MultiDelegatorRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no multi project delegators found"));
        }
        Ok(rows
            .into_iter()
            .map(|row| MultiDelegator {
                wallet: row.wallet,
                eoa: row.eoa,
                project_count: row.project_count,
                projects: row.projects,
            })
            .collect())
    }
}

async fn ensure_schema(
    admin: &clickhouse::Client,
    client: &clickhouse::Client,
    database: &str,
) -> Result<(), Error> {
    let create_db = format!("create database if not exists {}", database);
    admin.query(&create_db).execute().await?;
    let stmts = [
        "create table if not exists oracle_snapshots(ts DateTime64(3), ticker String, tx_id String) engine=MergeTree order by (ticker, ts)",
        "create table if not exists wallet_balances(ts DateTime64(3), ticker String, wallet String, eoa String, amount String, tx_id String) engine=ReplacingMergeTree order by (ticker, wallet, ts)",
        "create table if not exists wallet_delegations(ts DateTime64(3), wallet String, payload String) engine=ReplacingMergeTree order by (wallet, ts)",
        "create table if not exists flp_positions(ts DateTime64(3), ticker String, wallet String, eoa String, project String, factor UInt32, amount String) engine=ReplacingMergeTree order by (project, wallet, ts)",
        "create table if not exists delegation_mappings(ts DateTime64(3), height UInt32, tx_id String, wallet_from String, wallet_to String, factor UInt32) engine=ReplacingMergeTree order by (height, tx_id, wallet_from, wallet_to)",
    ];
    for stmt in stmts {
        client.query(stmt).execute().await?;
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
        client.query(stmt).execute().await?;
    }
    Ok(())
}

fn aggregate_totals(rows: &[FlpPositionRow]) -> Vec<ProjectTotal> {
    let mut map = BTreeMap::new();
    for row in rows {
        let entry = map.entry(row.ticker.clone()).or_insert(ProjectTotal {
            ticker: row.ticker.clone(),
            amount: 0.0,
            ar_amount: 0.0,
            delegators_count: 0,
        });
        entry.amount += row.amount.parse::<f64>().unwrap_or(0.0);
        entry.ar_amount += row.ar_amount.parse::<f64>().unwrap_or(0.0);
        entry.delegators_count += 1;
    }
    map.into_values().collect()
}

#[derive(Row, serde::Deserialize)]
struct FlpPositionRow {
    #[allow(dead_code)]
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    ticker: String,
    wallet: String,
    eoa: String,
    #[allow(dead_code)]
    project: String,
    factor: u32,
    amount: String,
    ar_amount: String,
}

#[derive(Row, serde::Deserialize)]
struct IdentityRow {
    wallet: String,
    eoa: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
}

impl From<IdentityRow> for IdentityLink {
    fn from(value: IdentityRow) -> Self {
        IdentityLink {
            wallet: value.wallet,
            eoa: value.eoa,
            ts: value.ts,
        }
    }
}

#[derive(Serialize)]
pub struct ProjectSnapshot {
    pub project: String,
    pub ts: DateTime<Utc>,
    pub totals: Vec<ProjectTotal>,
    pub delegators: Vec<Delegator>,
}

#[derive(Serialize, Clone)]
pub struct ProjectTotal {
    pub ticker: String,
    pub amount: f64,
    pub delegators_count: u32,
    pub ar_amount: f64,
}

#[derive(Serialize, Clone)]
pub struct Delegator {
    pub wallet: String,
    pub eoa: String,
    pub ticker: String,
    pub factor: u32,
    pub amount: String,
    pub ar_amount: String,
}

#[derive(Serialize, Clone)]
pub struct IdentityLink {
    pub wallet: String,
    pub eoa: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub ts: DateTime<Utc>,
}

#[derive(Row, serde::Deserialize, Serialize, Clone)]
pub struct OracleSnapshot {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub ticker: String,
    pub tx_id: String,
    pub total: f64,
    pub delegators: u64,
}

#[derive(Row, serde::Deserialize)]
struct DelegationMappingRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    height: u32,
    tx_id: String,
    wallet_from: String,
    wallet_to: String,
    factor: u32,
}

#[derive(Serialize, Clone)]
pub struct DelegationMappingHistory {
    pub ts: DateTime<Utc>,
    pub height: u32,
    pub tx_id: String,
    pub wallet: String,
    pub preferences: Vec<DelegationPreference>,
}

#[derive(Serialize, Clone)]
pub struct DelegationPreference {
    pub wallet_to: String,
    pub factor: u32,
}

#[derive(Row, serde::Deserialize)]
struct DelegationHeightRow {
    height: u32,
    tx_id: String,
}

#[derive(Serialize, Clone)]
pub struct DelegationHeight {
    pub height: u32,
    pub tx_id: String,
}

#[derive(Row, serde::Deserialize)]
struct MultiDelegatorRow {
    wallet: String,
    eoa: String,
    project_count: u64,
    projects: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct MultiDelegator {
    pub wallet: String,
    pub eoa: String,
    pub project_count: u64,
    pub projects: Vec<String>,
}
