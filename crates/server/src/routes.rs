use crate::{
    errors::ServerError,
    indexer::{
        AtlasIndexerClient, DelegationHeight, DelegationMappingHistory, ExplorerBlock,
        ExplorerDayStats, MultiDelegator, ProjectCycleTotal,
    },
};
use anyhow::anyhow;
use axum::{
    Json,
    extract::{Path, Query},
};
use chrono::{NaiveDate, Utc};
use common::{gql::OracleStakers, minting::get_flp_own_minting_report, projects::Project};
use flp::csv_parser::parse_flp_balances_setting_res;
use flp::json_parser::parse_own_minting_report;
use flp::wallet::get_wallet_delegations;
use serde_json::{Value, json};
use std::collections::HashMap;

pub async fn handle_route() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "atlas-server",
        "version": env!("CARGO_PKG_VERSION")
    }))
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
    let rows = client
        .mainnet_messages_by_tag(protocol.as_deref(), &key, &value, limit)
        .await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

pub async fn get_mainnet_indexing_info() -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new().await?;
    let rows = client.mainnet_indexing_info().await?;
    Ok(Json(serde_json::to_value(&rows)?))
}

fn parse_protocol(value: Option<&String>) -> Result<Option<String>, ServerError> {
    if let Some(p) = value {
        let normalized = p.trim().to_ascii_uppercase();
        if normalized.is_empty() {
            return Ok(None);
        }
        if normalized != "A" && normalized != "B" {
            return Err(ServerError::from(anyhow!("invalid protocol (expected A or B)")));
        }
        return Ok(Some(normalized));
    }
    Ok(None)
}
