use crate::core::utils::DATAITEMS_ADDRESS;
use axum::{extract::Path, response::Json};
use serde_json::{Value, json};

pub async fn handle_route() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "version": env!("CARGO_PKG_VERSION"),
        "address": DATAITEMS_ADDRESS
    }))
}
