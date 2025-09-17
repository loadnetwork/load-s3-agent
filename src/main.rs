use crate::core::server::{
    OBJECT_SIZE_LIMIT, SERVER_PORT, handle_get_bucket_registry, handle_post_dataitem,
    handle_private_file, handle_route, handle_storage_stats, serve_dataitem, upload_file,
};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use dotenvy::dotenv;
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};

mod core;

#[tokio::main]
async fn main() {
    // Load environment variables from a .env file if present
    dotenv().ok();

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let router = Router::new()
        .route("/", get(handle_route))
        .route("/stats", get(handle_storage_stats))
        .route("/upload", post(upload_file))
        .route("/upload/private", post(handle_private_file))
        .route("/post/{id}", post(handle_post_dataitem))
        .route("/registry/{bucket_name}", get(handle_get_bucket_registry))
        .route("/{id}", get(serve_dataitem))
        .layer(DefaultBodyLimit::max(OBJECT_SIZE_LIMIT))
        .layer(RequestBodyLimitLayer::new(OBJECT_SIZE_LIMIT))
        .layer(cors);

    // Use SERVER_PORT from env if set, otherwise default to the constant
    let port = std::env::var("SERVER_PORT").unwrap_or_else(|_| SERVER_PORT.to_string());

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await.unwrap();
    println!("Server running on PORT: {port}");
    axum::serve(listener, router).await.unwrap();
}
