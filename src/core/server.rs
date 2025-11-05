use crate::core::{
    bundler::post_dataitem,
    metadata::{
        DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE, TagQueryPagination, decode_tag_query_cursor,
        query_dataitems_by_tags,
    },
    registry::get_bucket_registry,
    s3::{
        get_bucket_stats, get_dataitem_url, store_dataitem, store_lcp_priv_bucket_dataitem,
        store_signed_dataitem,
    },
    utils::{get_env_var, is_valid_api_key},
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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use crate::core::utils::{OBJECT_SIZE_LIMIT, SERVER_PORT};

#[derive(Deserialize)]
pub(crate) struct TagFilter {
    key: String,
    value: String,
}

#[derive(Deserialize)]
pub(crate) struct TagQueryRequest {
    filters: Vec<TagFilter>,
    #[serde(default)]
    first: Option<usize>,
    #[serde(default)]
    after: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct UploadTag {
    key: String,
    value: String,
}

#[derive(Serialize)]
pub(crate) struct TagQueryItem {
    dataitem_id: String,
    content_type: String,
    created_at: String,
}

pub async fn handle_route() -> Json<Value> {
    Json(serde_json::json!({
        "status": "running",
        "name": "load-s3-agent",
        "version": env!("CARGO_PKG_VERSION"),
        "address": crate::core::utils::DATAITEMS_ADDRESS,
        "object_size_limit": crate::core::utils::OBJECT_SIZE_LIMIT,
        "presigned_url_expiry": crate::core::utils::PRESIGNED_URL_EXPIRY,
        "data_protocol": crate::core::utils::STORAGE_PROVIDER_NAME,
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

pub async fn handle_query_tags(
    Json(payload): Json<TagQueryRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if payload.filters.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "filters array must not be empty"})),
        ));
    }

    let filters: Vec<(String, String)> =
        payload.filters.iter().map(|f| (f.key.clone(), f.value.clone())).collect();

    let requested_first = payload.first.unwrap_or(DEFAULT_PAGE_SIZE);
    if requested_first == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "first must be greater than 0"})),
        ));
    }
    if requested_first > MAX_PAGE_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": format!("first must not exceed {MAX_PAGE_SIZE}")})),
        ));
    }
    let first = requested_first;

    let after_cursor = match payload.after.as_deref() {
        Some(cursor) => Some(decode_tag_query_cursor(cursor).map_err(|err| {
            (StatusCode::BAD_REQUEST, Json(json!({"error": format!("invalid cursor: {err}")})))
        })?),
        None => None,
    };

    let pagination = TagQueryPagination { first, after: after_cursor };

    match query_dataitems_by_tags(&filters, &pagination).await {
        Ok(page) => {
            let items: Vec<TagQueryItem> = page
                .items
                .into_iter()
                .map(|record| TagQueryItem {
                    dataitem_id: record.dataitem_id,
                    content_type: record.content_type,
                    created_at: record.created_at.to_rfc3339(),
                })
                .collect();

            Ok(Json(json!({
                "success": true,
                "count": items.len(),
                "items": items,
                "page_info": {
                    "has_next_page": page.has_more,
                    "next_cursor": page.next_cursor
                }
            })))
        }
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("failed to query tags: {err}")})),
        )),
    }
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

    let api_keys: Vec<String> = server_api_keys.split(',').map(|s| s.trim().to_string()).collect();

    if !api_keys.contains(&token.to_string()) {
        let potential_valid_load_acc = is_valid_api_key(&token).await.map_err(|_| {
            (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid load_acc key"})))
        })?;

        if !potential_valid_load_acc {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "invalid API key"
                })),
            ));
        }
    }

    let mut file_data: Option<Vec<u8>> = None;
    let mut content_type: Option<String> = None;
    let mut extra_tags: Vec<UploadTag> = Vec::new();

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
            "tags" => {
                let text = field.text().await.map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": "failed to read tags field"
                        })),
                    )
                })?;

                let parsed: Vec<UploadTag> = serde_json::from_str(&text).map_err(|_| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": "invalid tags payload, expected JSON array of objects with key/value"
                        })),
                    )
                })?;

                extra_tags = parsed;
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

    let is_signed =
        headers.get("signed").and_then(|h| h.to_str().ok()).map(|s| s == "true").unwrap_or(false);

    if is_signed && !extra_tags.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "custom tags are not supported when uploading signed dataitems, the dataitems tags will be extracted and appied instead"
            })),
        ));
    }

    let extra_tag_pairs: Vec<(String, String)> =
        extra_tags.iter().map(|tag| (tag.key.clone(), tag.value.clone())).collect();

    let result = if is_signed {
        store_signed_dataitem(file_bytes).await
    } else {
        store_dataitem(file_bytes, content_type_str, &extra_tag_pairs).await
    };

    match result {
        Ok(dataitem_id) => Ok(Json(json!({
            "success": true,
            "dataitem_id": dataitem_id,
            "custom_tags": extra_tags,
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

pub async fn handle_private_file(
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

    let load_acc = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "invalid Authorization header format. Expected 'Bearer <token>'"
            })),
        )
    })?;

    let bucket_name = headers
        .get("bucket_name")
        .or_else(|| headers.get("bucket-name"))
        .or_else(|| headers.get("x-bucket-name"))
        .or_else(|| headers.get("bucketname"))
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "missing bucket_name header"
                })),
            )
        })?;

    let dataitem_name = headers
        .get("x-dataitem-name")
        .or_else(|| headers.get("dataitem-name"))
        .or_else(|| headers.get("dataitemname"))
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    let folder_name = headers.get("x-folder-name").and_then(|h| h.to_str().ok()).unwrap_or("");

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

    let is_signed =
        headers.get("signed").and_then(|h| h.to_str().ok()).map(|s| s == "true").unwrap_or(false);

    // private dataitems store
    // supports signed (ANS-104 ready) and unsigned (raw dataitem's data) data ingress
    match store_lcp_priv_bucket_dataitem(
        file_bytes,
        content_type_str,
        bucket_name,
        folder_name,
        load_acc,
        dataitem_name,
        is_signed,
    )
    .await
    {
        Ok(dataitem_id) => Ok(Json(json!({
            "success": true,
            "dataitem_id": dataitem_id,
            "dataitem_name": dataitem_name,
            "folder_name": folder_name,
            "is_signed": is_signed,
            "message": "file uploaded to private bucket successfully"
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("failed to store file: {}", e)
            })),
        )),
    }
}

pub async fn handle_post_dataitem(
    headers: HeaderMap,
    Path(dataitem_id): Path<String>,
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

    let api_keys: Vec<String> = server_api_keys.split(',').map(|s| s.trim().to_string()).collect();

    if !api_keys.contains(&token.to_string()) {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "invalid API key"
            })),
        ));
    }

    match post_dataitem(dataitem_id.clone()).await {
        Ok(response) => Ok(Json(json!({
            "success": true,
            "dataitem_id": dataitem_id,
            "bundler_response": response,
            "message": "dataitem posted to arweave successfully"
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("failed to post dataitem: {}", e)
            })),
        )),
    }
}

pub async fn handle_get_bucket_registry(
    headers: HeaderMap,
    Path(bucket_name): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let auth_header =
        headers.get("authorization").and_then(|h| h.to_str().ok()).ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(json!({"error": "missing Authorization header"})))
        })?;

    let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid Authorization header format"})))
    })?;

    let aws_secret = get_env_var("REGISTRY_SECRET_KEY").map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "server configuration error"})))
    })?;

    if token != aws_secret {
        return Err((StatusCode::UNAUTHORIZED, Json(json!({"error": "invalid API key"}))));
    }

    match get_bucket_registry(&bucket_name) {
        Ok(registry_entries) => Ok(Json(json!({
            "success": true,
            "bucket_name": bucket_name,
            "entries": registry_entries
        }))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("failed to get registry: {}", e)})),
        )),
    }
}
