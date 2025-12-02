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
    pub fn new() -> Result<Self, Error> {
        let url = get_env_var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
        let user = get_env_var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
        let password = get_env_var("CLICKHOUSE_PASSWORD").unwrap_or_default();
        let database =
            get_env_var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "atlas_oracles".into());
        let client = clickhouse::Client::default()
            .with_url(url)
            .with_user(user)
            .with_password(password)
            .with_database(database);
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
