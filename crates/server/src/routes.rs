use crate::{errors::ServerError, indexer::AtlasIndexerClient};
use axum::{Json, extract::Path};
use common::gql::OracleStakers;
use flp::set_balances::parse_flp_balances_setting_res;
use flp::wallet::get_wallet_delegations;
use serde_json::Value;

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
