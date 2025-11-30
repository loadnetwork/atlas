use std::{env, time::Duration};

#[derive(Clone)]
pub struct Config {
    pub clickhouse_url: String,
    pub clickhouse_user: String,
    pub clickhouse_password: String,
    pub clickhouse_database: String,
    pub interval: Duration,
    pub concurrency: usize,
    pub tickers: Vec<String>,
}

impl Config {
    pub fn load() -> Self {
        let clickhouse_url =
            env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
        let clickhouse_user = env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
        let clickhouse_password = env::var("CLICKHOUSE_PASSWORD").unwrap_or_default();
        let clickhouse_database =
            env::var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "default".into());
        let interval = env::var("ORACLE_REFRESH_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(300));
        let concurrency = env::var("DELEGATION_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(16);
        let tickers = env::var("ORACLE_TICKERS")
            .unwrap_or_else(|_| "usds,dai,steth".into())
            .split(',')
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .collect();
        Config {
            clickhouse_url,
            clickhouse_user,
            clickhouse_password,
            clickhouse_database,
            interval,
            concurrency,
            tickers,
        }
    }
}
