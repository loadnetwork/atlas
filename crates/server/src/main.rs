use crate::routes::{
    get_ar_wallet_identity, get_eoa_wallet_identity, get_flp_snapshot_handler,
    get_oracle_data_handler, get_oracle_feed, get_wallet_delegations_handler, handle_route,
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
        .route("/oracle/{ticker}", get(get_oracle_data_handler))
        .route("/oracle/feed/{ticker}", get(get_oracle_feed))
        // returns the direct delegation data per FLP ID: LSTs + AR -- factored data
        .route("/flp/delegators/{project}", get(get_flp_snapshot_handler))
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
