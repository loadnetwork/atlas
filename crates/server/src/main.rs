use crate::routes::{
    get_all_projects_metadata_handler, get_ar_wallet_identity, get_delegation_mapping_heights,
    get_ao_token_messages_by_tag, get_ao_token_tx, get_ao_token_txs, get_eoa_wallet_identity,
    get_explorer_blocks, get_explorer_day_stats, get_explorer_recent_days,
    get_flp_own_minting_report_handler, get_flp_snapshot_handler, get_mainnet_block_messages,
    get_mainnet_explorer_blocks, get_mainnet_explorer_day_stats,
    get_mainnet_explorer_recent_days, get_mainnet_indexing_info, get_mainnet_messages_by_tag,
    get_mainnet_recent_messages, get_multi_project_delegators, get_oracle_data_handler,
    get_oracle_feed, get_project_cycle_totals, get_wallet_delegation_mappings_history,
    get_wallet_delegations_handler, handle_route,
};
use axum::{Router, extract::DefaultBodyLimit, routing::get};
use common::env::get_env_var;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};

const REQ_SIZE_LIMIT: usize = 50 * 1024 * 1024; // 50 MB

mod errors;
mod indexer;
mod routes;

#[tokio::main]
async fn main() {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let router = Router::new()
        .route("/", get(handle_route))
        // wallet operations
        .route(
            "/wallet/delegations/{address}",
            get(get_wallet_delegations_handler),
        )
        .route("/wallet/identity/eoa/{eoa}", get(get_eoa_wallet_identity))
        .route(
            "/wallet/identity/ar-wallet/{address}",
            get(get_ar_wallet_identity),
        )
        .route(
            "/wallet/delegation-mappings/{address}",
            get(get_wallet_delegation_mappings_history),
        )
        .route(
            "/delegation-mappings/heights",
            get(get_delegation_mapping_heights),
        )
        .route("/flp/delegators/multi", get(get_multi_project_delegators))
        .route("/oracle/{ticker}", get(get_oracle_data_handler))
        .route("/oracle/feed/{ticker}", get(get_oracle_feed))
        // returns the direct delegation data per FLP ID: LSTs + AR -- factored data
        .route("/flp/delegators/{project}", get(get_flp_snapshot_handler))
        .route("/flp/{project}/cycles", get(get_project_cycle_totals))
        .route(
            "/flp/minting/{project}",
            get(get_flp_own_minting_report_handler),
        )
        .route("/flp/metadata/all", get(get_all_projects_metadata_handler))
        .route("/explorer/blocks", get(get_explorer_blocks))
        .route("/explorer/day", get(get_explorer_day_stats))
        .route("/explorer/days", get(get_explorer_recent_days))
        // mainnet (ao.N.1)
        .route("/mainnet/explorer/blocks", get(get_mainnet_explorer_blocks))
        .route("/mainnet/explorer/day", get(get_mainnet_explorer_day_stats))
        .route("/mainnet/explorer/days", get(get_mainnet_explorer_recent_days))
        .route("/mainnet/messages/recent", get(get_mainnet_recent_messages))
        .route("/mainnet/messages/block/{height}", get(get_mainnet_block_messages))
        .route("/mainnet/messages/tags", get(get_mainnet_messages_by_tag))
        .route("/mainnet/info", get(get_mainnet_indexing_info))
        .route("/token/ao/txs", get(get_ao_token_txs))
        .route("/token/ao/txs/{msg_id}", get(get_ao_token_tx))
        .route("/token/ao/txs/tags", get(get_ao_token_messages_by_tag))
        .layer(DefaultBodyLimit::max(REQ_SIZE_LIMIT))
        .layer(RequestBodyLimitLayer::new(REQ_SIZE_LIMIT))
        .layer(cors);
    // 12 titans :D
    let port = get_env_var("SERVER_PORT").unwrap_or_else(|_| "1212".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    println!("Server running on PORT: {port}");
    axum::serve(listener, router).await.unwrap();
}
