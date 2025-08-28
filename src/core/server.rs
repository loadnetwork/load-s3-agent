use crate::core::{
    s3::{get_bucket_stats, get_dataitem_url, store_dataitem},
    utils::get_env_var,
};
use axum::{
    Json,
    body::Body,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_extra::extract::Multipart;
use headers::HeaderMap;
use serde_json::{Value, json};

pub use crate::core::utils::{OBJECT_SIZE_LIMIT, SERVER_PORT};

pub async fn handle_route() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "load-s3-agent",
        "version": env!("CARGO_PKG_VERSION"),
        "address": crate::core::utils::DATAITEMS_ADDRESS,
        "object_size_limit": crate::core::utils::OBJECT_SIZE_LIMIT,
        "presigned_url_expiry": crate::core::utils::PRESIGNED_URL_EXPIRY,
        "data_protocol": crate::core::utils::DATA_PROTOCOL_NAME,
        "hyperbeam_node_url": crate::core::utils::HYPERBEAM_NODE_URL,
    }))
}

pub async fn handle_storage_stats() -> Json<Value> {
    let stats = get_bucket_stats().await.unwrap_or_default();

    Json(serde_json::json!({
        "total_dataitems_count": stats.0,
        "total_dataitems_size": stats.1
    }))
}

pub async fn serve_dataitem(Path(dataitem_id): Path<String>) -> impl IntoResponse {
    match get_dataitem_url(&dataitem_id).await {
        Ok(url) => Response::builder()
            .status(StatusCode::FOUND)
            .header("location", url)
            .body(Body::empty())
            .unwrap(),
        Err(e) => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"error": "{e}"}}"#)))
            .unwrap(),
    }
}

pub async fn upload_file(
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let auth_header =
        headers.get("authorization").and_then(|h| h.to_str().ok()).ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "missing Authorization header"
                })),
            )
        })?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "invalid Authorization header format. Expected 'Bearer <token>'"
            })),
        )
    })?;

    let server_api_keys = get_env_var("SERVER_API_KEYS").map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": "server configuration error"
            })),
        )
    })?;

    let api_keys: Vec<String> = server_api_keys
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    if !api_keys.contains(&token.to_string()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "invalid API key"
            })),
        ));
    }

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid multipart data"
            })),
        )
    })? {
        let field_name = field.name().unwrap_or("");

        match field_name {
            "file" => {
                content_type = field.content_type().map(|ct| ct.to_string());
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|_| {
                            (
                                StatusCode::BAD_REQUEST,
                                Json(json!({
                                    "error": "failed to read file data"
                                })),
                            )
                        })?
                        .to_vec(),
                );
            }
            "content_type" => {
                if content_type.is_none() {
                    content_type = Some(field.text().await.map_err(|_| {
                        (
                            StatusCode::BAD_REQUEST,
                            Json(json!({
                                "error": "failed to read content type"
                            })),
                        )
                    })?);
                }
            }
            _ => {
                // skip
            }
        }
    }

    let file_bytes = file_data.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "no file data provided"
            })),
        )
    })?;

    if file_bytes.len() > OBJECT_SIZE_LIMIT {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({"error": format!("file size exceeds limit - {OBJECT_SIZE_LIMIT} bytes")})),
        ));
    }

    let content_type_str = content_type.as_deref().unwrap_or("application/octet-stream");

    match store_dataitem(file_bytes, content_type_str).await {
        Ok(dataitem_id) => Ok(Json(json!({
            "success": true,
            "dataitem_id": dataitem_id,
            "message": "file uploaded successfully"
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("failed to store file: {}", e)
            })),
        )),
    }
}
