use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::get,
};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};
use crate::routes::handle_route;

const REQ_SIZE_LIMIT: usize = 50 * 1024 * 1024; // 50 MB

mod routes;

#[tokio::main]
async fn main() {

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let router = Router::new()
        .route("/", get(handle_route))
        .layer(DefaultBodyLimit::max(REQ_SIZE_LIMIT))
        .layer(RequestBodyLimitLayer::new(REQ_SIZE_LIMIT))
        .layer(cors);

    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "3000".to_string());
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    println!("Server running on PORT: {port}");
    axum::serve(listener, router).await.unwrap();
}