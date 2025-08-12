use crate::core::server::handle_route;
use axum::{Router, routing::get};

mod core;

#[tokio::main]
async fn main() {
    let router = Router::new().route("/", get(handle_route));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, router).await.unwrap();
}
