use crate::core::server::{
    OBJECT_SIZE_LIMIT, SERVER_PORT, handle_post_dataitem, handle_route, handle_storage_stats,
    serve_dataitem, upload_file,
};
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use tower_http::{cors::CorsLayer, limit::RequestBodyLimitLayer};

mod core;

#[tokio::main]
async fn main() {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let router = Router::new()
        .route("/", get(handle_route))
        .route("/stats", get(handle_storage_stats))
        .route("/upload", post(upload_file))
        .route("/post/{id}", post(handle_post_dataitem))
        .route("/{id}", get(serve_dataitem))
        .layer(DefaultBodyLimit::max(OBJECT_SIZE_LIMIT))
        .layer(RequestBodyLimitLayer::new(OBJECT_SIZE_LIMIT))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{SERVER_PORT}")).await.unwrap();
    println!("Server running on PORT: {SERVER_PORT}");
    axum::serve(listener, router).await.unwrap();
}
