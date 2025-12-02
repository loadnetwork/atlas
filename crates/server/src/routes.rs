use crate::{errors::ServerError, indexer::AtlasIndexerClient};
use axum::{Json, extract::Path};
use common::gql::OracleStakers;
use flp::set_balances::parse_flp_balances_setting_res;
use flp::wallet::get_wallet_delegations;
use serde_json::{Value, json};

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
    let client = AtlasIndexerClient::new()?;
    let snapshot = client.latest_project_snapshot(&project).await?;
    Ok(Json(serde_json::to_value(snapshot)?))
}

pub async fn get_eoa_wallet_identity(Path(eoa): Path<String>) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new()?;
    let identities = client.eoa_identity_history(&eoa).await?;
    Ok(Json(serde_json::to_value(&identities)?))
}

pub async fn get_ar_wallet_identity(
    Path(address): Path<String>,
) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new()?;
    let identities = client.wallet_identity_history(&address).await?;
    Ok(Json(serde_json::to_value(&identities)?))
}

pub async fn get_oracle_feed(Path(ticker): Path<String>) -> Result<Json<Value>, ServerError> {
    let client = AtlasIndexerClient::new()?;
    let feed = client.oracle_snapshot_feed(&ticker, 25).await?;
    let metadata = OracleStakers::new(&ticker).oracle.metadata()?;
    let res = json!({
        "oracle_pid": metadata.ao_pid,
        "oracle_evm_address": metadata.evm_address,
        "recent_indexed_feeds": feed
    });
    Ok(Json(res))
}
