use crate::{
    errors::ServerError,
    indexer::{
        AoTokenMessage, AtlasIndexerClient, DelegationHeight, DelegationMappingHistory,
        ExplorerBlock, ExplorerDayStats, MultiDelegator, ProjectCycleTotal,
    },
};
use anyhow::anyhow;
use axum::{
    Json,
    extract::{Path, Query},
};
use chrono::{NaiveDate, Utc};
use common::{
    env::get_env_var, gql::OracleStakers, minting::get_flp_own_minting_report, projects::Project,
};
use flp::csv_parser::parse_flp_balances_setting_res;
use flp::json_parser::parse_own_minting_report;
use flp::wallet::get_wallet_delegations;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::{fs, io::ErrorKind};

#[derive(Deserialize, Serialize, Default)]
struct AtlasConfig {
    #[serde(default)]
    indexers: AtlasIndexersConfig,
    #[serde(rename = "PRIMARY_ARWEAVE_GATEWAY", alias = "primary_arweave_gateway")]
    primary_arweave_gateway: Option<String>,
}

#[derive(Deserialize, Serialize, Default)]
struct AtlasIndexersConfig {
    ao: Option<bool>,
    pi: Option<bool>,
    flp: Option<bool>,
    explorer: Option<bool>,
    mainnet: Option<bool>,
}

fn load_atlas_config() -> Option<AtlasConfig> {
    let path = get_env_var("ATLAS_CONFIG").ok();
    if let Some(path) = path {
        return read_atlas_config(&path);
    }
    read_atlas_config("atlas.toml").or_else(|| read_atlas_config("../atlas.toml"))
}

fn read_atlas_config(path: &str) -> Option<AtlasConfig> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return None,
        Err(err) => {
            eprintln!("failed to read atlas config {path}: {err}");
            return None;
        }
    };
    match toml::from_str::<AtlasConfig>(&contents) {
        Ok(config) => Some(config),
        Err(err) => {
            eprintln!("failed to parse atlas config {path}: {err}");
            None
        }
    }
}

pub async fn handle_route() -> Json<Value> {
    let config = load_atlas_config();
    Json(serde_json::json!({
        "status": "running",
        "name": "atlas-server",
        "version": env!("CARGO_PKG_VERSION"),
        "config": config
    }))
}

pub async fn parse_set_balance_report(Path(id): Path<String>) -> Result<Json<Value>, ServerError> {
    let res = parse_flp_balances_setting_res(&id)?;
    Ok(Json(serde_json::to_value(&res)?))
}

pub async fn get_wallet_delegations_handler(
    Path(address): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let res = get_wallet_delegations(&address).unwrap();
    Ok(Json(serde_json::to_value(&res)?))
}

pub async fn get_oracle_data_handler(
    Path(ticker): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let oracle = OracleStakers::new(&ticker).build()?.send()?;
    let last_update = oracle.last_update()?;
    let set_balances_parsed_data = parse_flp_balances_setting_res(&last_update)?;
    Ok(Json(serde_json::to_value(&set_balances_parsed_data)?))
}

pub async fn get_flp_snapshot_handler(
    Path(project): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let snapshot = client.latest_project_snapshot(&project).await?;
    Ok(Json(serde_json::to_value(snapshot)?))
}

pub async fn get_eoa_wallet_identity(Path(eoa): Path<String>) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let identities = client.eoa_identity_history(&eoa).await?;
    Ok(Json(serde_json::to_value(&identities)?))
}

pub async fn get_ar_wallet_identity(
    Path(address): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let identities = client.wallet_identity_history(&address).await?;
    Ok(Json(serde_json::to_value(&identities)?))
}

pub async fn get_oracle_feed(Path(ticker): Path<String>) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let feed = client.oracle_snapshot_feed(&ticker, 25).await?;
    let metadata = OracleStakers::new(&ticker).oracle.metadata()?;
    let res = json!({
        "oracle_pid": metadata.ao_pid,
        "oracle_evm_address": metadata.evm_address,
        "recent_indexed_feeds": feed
    });
    Ok(Json(res))
}

pub async fn get_wallet_delegation_mappings_history(
    Path(address): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let history: Vec<DelegationMappingHistory> =
        client.wallet_delegation_mappings(&address).await?;
    Ok(Json(serde_json::to_value(&history)?))
}

pub async fn get_delegation_mapping_heights(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(25);
    let client = AtlasIndexerClient::new().await?;
    let rows: Vec<DelegationHeight> = client.latest_delegation_heights(limit).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_multi_project_delegators(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let client = AtlasIndexerClient::new().await?;
    let rows: Vec<MultiDelegator> = client.multi_project_delegators(limit).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_project_cycle_totals(
    Path(project): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(25);
    let ticker = params.get("ticker").cloned();
    let client = AtlasIndexerClient::new().await?;
    let rows: Vec<ProjectCycleTotal> = client
        .project_cycle_totals(&project, ticker.as_deref(), limit)
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_flp_own_minting_report_handler(
    Path(project): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let report_id: String = get_flp_own_minting_report(&project)?;
    let report = parse_own_minting_report(&report_id)?;
    Ok(Json(serde_json::to_value(&report)?))
}

pub async fn get_all_projects_metadata_handler() -> Result<Json<Value>, ServerError> {
    let projects = Project::get_all();
    Ok(Json(serde_json::to_value(&projects)?))
}

pub async fn get_explorer_blocks(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let client = AtlasIndexerClient::new().await?;
    let rows: Vec<ExplorerBlock> = client.latest_explorer_blocks(limit).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_explorer_day_stats(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let day_str = params
        .get("day")
        .map(|s| s.to_string())
        .unwrap_or_else(|| Utc::now().date_naive().to_string());
    let day = NaiveDate::parse_from_str(&day_str, "%Y-%m-%d")
        .map_err(|_| ServerError::from(anyhow!("invalid day format (expected YYYY-MM-DD)")))?;
    let client = AtlasIndexerClient::new().await?;
    let stats: ExplorerDayStats = client.daily_explorer_stats(day).await?;
    Ok(Json(serde_json::to_value(&stats)?))
}

pub async fn get_explorer_recent_days(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(7);
    let client = AtlasIndexerClient::new().await?;
    let rows = client.recent_explorer_days(limit).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_explorer_blocks(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let client = AtlasIndexerClient::new().await?;
    let rows = client.mainnet_explorer_blocks(limit).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_explorer_day_stats(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let day_str = params
        .get("day")
        .map(|s| s.to_string())
        .unwrap_or_else(|| Utc::now().date_naive().to_string());
    let day = NaiveDate::parse_from_str(&day_str, "%Y-%m-%d")
        .map_err(|_| ServerError::from(anyhow!("invalid day format (expected YYYY-MM-DD)")))?;
    let client = AtlasIndexerClient::new().await?;
    let stats = client.mainnet_daily_explorer_stats(day).await?;
    Ok(Json(serde_json::to_value(&stats)?))
}

pub async fn get_mainnet_explorer_recent_days(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(7);
    let client = AtlasIndexerClient::new().await?;
    let rows = client.mainnet_recent_explorer_days(limit).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_recent_messages(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let protocol = parse_protocol(params.get("protocol"))?;
    let client = AtlasIndexerClient::new().await?;
    let rows = client
        .recent_mainnet_messages(protocol.as_deref(), limit)
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_block_messages(
    Path(height): Path<u32>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(500);
    let protocol = parse_protocol(params.get("protocol"))?;
    let client = AtlasIndexerClient::new().await?;
    let rows = client
        .block_mainnet_messages(protocol.as_deref(), height, limit)
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_messages_by_tag(
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let protocol = parse_protocol(params.get("protocol"))?;
    let key = params
        .get("key")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ServerError::from(anyhow!("missing tag key")))?;
    let value = params
        .get("value")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ServerError::from(anyhow!("missing tag value")))?;
    let client = AtlasIndexerClient::new().await?;
    let tag_keys = build_tag_key_variants(protocol.as_deref(), &key);
    if tag_keys.is_empty() {
        return Err(ServerError::from(anyhow!("invalid tag key")));
    }
    let rows = client
        .mainnet_messages_by_tag(protocol.as_deref(), &tag_keys, &value, limit)
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_indexing_info() -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let rows = client.mainnet_indexing_info().await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_ao_token_txs(
    Path(token): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let token = parse_token(&token)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let offset = params
        .get("offset")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let source = params
        .get("source")
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| v == "transfer" || v == "process");
    let action = params
        .get("action")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let recipient = params
        .get("recipient")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let sender = params
        .get("sender")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let order = params
        .get("order")
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| v == "asc" || v == "desc");
    let min_qty = parse_amount_param(params.get("min_amount"))?;
    let max_qty = parse_amount_param(params.get("max_amount"))?;
    let from_ts = parse_u64_param(params.get("from_ts"))?;
    let to_ts = parse_u64_param(params.get("to_ts"))?;
    let block_min = parse_u32_param(params.get("block_min"))?;
    let block_max = parse_u32_param(params.get("block_max"))?;
    let client = AtlasIndexerClient::new().await?;
    let rows: Vec<AoTokenMessage> = client
        .ao_token_messages(
            &token,
            source.as_deref(),
            action.as_deref(),
            min_qty.as_deref(),
            max_qty.as_deref(),
            from_ts,
            to_ts,
            block_min,
            block_max,
            recipient.as_deref(),
            sender.as_deref(),
            order.as_deref(),
            limit,
            offset,
        )
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_ao_token_tx(
    Path((token, msg_id)): Path<(String, String)>,
) -> Result<Json<Value>, ServerError> {
    let token = parse_token(&token)?;
    let client = AtlasIndexerClient::new().await?;
    let rows = client.ao_token_message_by_id(&token, &msg_id).await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_ao_token_messages_by_tag(
    Path(token): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let token = parse_token(&token)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(100);
    let source = params
        .get("source")
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| v == "transfer" || v == "process");
    let key = params
        .get("key")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ServerError::from(anyhow!("missing tag key")))?;
    let value = params
        .get("value")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ServerError::from(anyhow!("missing tag value")))?;
    let client = AtlasIndexerClient::new().await?;
    let rows = client
        .ao_token_messages_by_tag(&token, source.as_deref(), &key, &value, limit)
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_ao_token_indexing_info(
    Path(token): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let token = parse_token(&token)?;
    let client = AtlasIndexerClient::new().await?;
    let info = client.ao_token_indexing_info(&token).await?;
    Ok(Json(serde_json::to_value(&info)?))
}

pub async fn get_ao_token_frequency(
    Path(token): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let token = parse_token(&token)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(25);
    let client = AtlasIndexerClient::new().await?;
    let info = client.ao_token_frequency(&token, limit).await?;
    Ok(Json(serde_json::to_value(&info)?))
}

pub async fn get_ao_token_richlist(
    Path(token): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Value>, ServerError> {
    let token = parse_token(&token)?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(25);
    let client = AtlasIndexerClient::new().await?;
    let info = client.ao_token_richlist(&token, limit).await?;
    Ok(Json(serde_json::to_value(&info)?))
}

fn parse_protocol(value: Option<&String>) -> Result<Option<String>, ServerError> {
    if let Some(p) = value {
        let normalized = p.trim().to_ascii_uppercase();
        if normalized.is_empty() {
            return Ok(None);
        }
        if normalized != "A" && normalized != "B" {
            return Err(ServerError::from(anyhow!(
                "invalid protocol (expected A or B)"
            )));
        }
        return Ok(Some(normalized));
    }
    Ok(None)
}

fn build_tag_key_variants(protocol: Option<&str>, key: &str) -> Vec<String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    let header = to_header_case(trimmed);
    match protocol {
        Some("A") => vec![lower],
        Some("B") => vec![header],
        _ => {
            if lower == header {
                vec![lower]
            } else {
                vec![lower, header]
            }
        }
    }
}

fn to_header_case(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    for (i, segment) in input.split('-').enumerate() {
        if i > 0 {
            result.push('-');
        }
        let mut chars = segment.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());
        }
        for c in chars {
            result.push(c.to_ascii_lowercase());
        }
    }
    result
}

fn parse_u64_param(value: Option<&String>) -> Result<Option<u64>, ServerError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed
        .parse::<u64>()
        .map_err(|_| ServerError::from(anyhow!("invalid u64 value")))?;
    Ok(Some(parsed))
}

fn parse_u32_param(value: Option<&String>) -> Result<Option<u32>, ServerError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = trimmed
        .parse::<u32>()
        .map_err(|_| ServerError::from(anyhow!("invalid u32 value")))?;
    Ok(Some(parsed))
}

fn parse_amount_param(value: Option<&String>) -> Result<Option<String>, ServerError> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_human_amount_to_raw(trimmed)?))
}

fn parse_human_amount_to_raw(input: &str) -> Result<String, ServerError> {
    let mut parts = input.split('.');
    let whole_part = parts.next().unwrap_or("");
    let frac_part = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(ServerError::from(anyhow!("invalid amount format")));
    }
    let whole = if whole_part.is_empty() {
        "0"
    } else {
        whole_part
    };
    if !whole.chars().all(|c| c.is_ascii_digit()) {
        return Err(ServerError::from(anyhow!("invalid amount format")));
    }
    if !frac_part.chars().all(|c| c.is_ascii_digit()) {
        return Err(ServerError::from(anyhow!("invalid amount format")));
    }
    if frac_part.len() > 12 {
        return Err(ServerError::from(anyhow!(
            "amount has more than 12 decimal places"
        )));
    }
    let mut raw = String::with_capacity(whole.len() + 12);
    raw.push_str(whole);
    raw.push_str(frac_part);
    for _ in 0..(12 - frac_part.len()) {
        raw.push('0');
    }
    let trimmed = raw.trim_start_matches('0');
    if trimmed.is_empty() {
        return Ok("0".to_string());
    }
    Ok(trimmed.to_string())
}

fn parse_token(value: &str) -> Result<String, ServerError> {
    let token = value.trim().to_ascii_lowercase();
    if token.is_empty() {
        return Err(ServerError::from(anyhow!("missing token")));
    }
    if token != "ao" && token != "pi" {
        return Err(ServerError::from(anyhow!("unsupported token")));
    }
    Ok(token)
}
