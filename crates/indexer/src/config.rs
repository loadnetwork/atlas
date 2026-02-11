use common::env::get_env_var;
use serde::Deserialize;
use std::{fs, io::ErrorKind, time::Duration};

#[derive(Clone)]
pub struct Config {
    pub clickhouse_url: String,
    pub clickhouse_user: String,
    pub clickhouse_password: String,
    pub clickhouse_database: String,
    pub interval: Duration,
    pub concurrency: usize,
    pub tickers: Vec<String>,
    pub indexers: IndexerConfig,
}

#[derive(Clone, Copy)]
pub struct IndexerConfig {
    pub ao: bool,
    pub pi: bool,
    pub explorer: bool,
    pub flp: bool,
    pub mainnet: bool,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            ao: true,
            pi: true,
            explorer: true,
            flp: true,
            mainnet: true,
        }
    }
}

#[derive(Deserialize, Default)]
struct FileConfig {
    #[serde(default)]
    indexers: FileIndexersConfig,
}

#[derive(Deserialize, Default)]
struct FileIndexersConfig {
    ao: Option<bool>,
    pi: Option<bool>,
    flp: Option<bool>,
    explorer: Option<bool>,
    mainnet: Option<bool>,
}

impl Config {
    pub fn load() -> Self {
        let clickhouse_url =
            get_env_var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
        let clickhouse_user = get_env_var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
        let clickhouse_password = get_env_var("CLICKHOUSE_PASSWORD").unwrap_or_default();
        let clickhouse_database =
            get_env_var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "atlas_oracles".into());
        let interval = get_env_var("ORACLE_REFRESH_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or_else(|| Duration::from_secs(300));
        let concurrency = get_env_var("DELEGATION_CONCURRENCY")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(16);
        let tickers = get_env_var("ORACLE_TICKERS")
            .unwrap_or_else(|_| "usds,dai,steth".into())
            .split(',')
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .collect();
        let mut config = Config {
            clickhouse_url,
            clickhouse_user,
            clickhouse_password,
            clickhouse_database,
            interval,
            concurrency,
            tickers,
            indexers: IndexerConfig::default(),
        };
        if let Some(file_config) = FileConfig::load() {
            config.indexers.apply(file_config.indexers);
        }
        config
    }
}

impl IndexerConfig {
    fn apply(&mut self, file: FileIndexersConfig) {
        if let Some(value) = file.ao {
            self.ao = value;
        }
        if let Some(value) = file.pi {
            self.pi = value;
        }
        if let Some(value) = file.explorer {
            self.explorer = value;
        }
        if let Some(value) = file.flp {
            self.flp = value;
        }
        if let Some(value) = file.mainnet {
            self.mainnet = value;
        }
    }
}

impl FileConfig {
    fn load() -> Option<Self> {
        let path = get_env_var("ATLAS_CONFIG").unwrap_or_else(|_| "atlas.toml".into());
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(err) if err.kind() == ErrorKind::NotFound => return None,
            Err(err) => {
                eprintln!("failed to read atlas config {path}: {err}");
                return None;
            }
        };
        match toml::from_str::<FileConfig>(&contents) {
            Ok(config) => Some(config),
            Err(err) => {
                eprintln!("failed to parse atlas config {path}: {err}");
                None
            }
        }
    }
}
