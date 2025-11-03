use anyhow::Error;
use dotenvy::dotenv;
use std::env;

use reqwest::{
    Client,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue},
};

pub(crate) const DATA_PROTOCOL_NAME: &str = "Load-S3";
pub(crate) const DATAITEMS_ADDRESS: &str = "2BBwe2pSXn_Tp-q_mHry0Obp88dc7L-eDIWx0_BUfD0";
pub(crate) const PRESIGNED_URL_EXPIRY: u64 = 3600;
pub const OBJECT_SIZE_LIMIT: usize = 250 * 1024 * 1024; // 250 MB
pub const INTERNAL_AUTH_SERVER: &str = "https://k8s.load-auth-service.load.network";
// ASCII values of `load-s3-agent`:
// 108+111+97+100+45+115+51+45+97+103+101+110+116 = 1247
// [^^]
pub const SERVER_PORT: &str = "1247";
pub const HYPERBEAM_NODE_URL: &str = "https://s3-node-1.load.network";

pub(crate) fn get_env_var(key: &str) -> Result<String, Error> {
    dotenv().ok();
    Ok(env::var(key)?)
}

pub(crate) async fn is_valid_api_key(load_acc_token: &str) -> Result<bool, reqwest::Error> {
    let url = format!("{INTERNAL_AUTH_SERVER}/internal/verify/{load_acc_token}");
    let server_auth = get_env_var("AUTH_SERVER_KEY").unwrap();

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("X-Load-Auth-Token", HeaderValue::from_str(&server_auth).unwrap());

    let client = Client::new();
    let response = client.get(&url).headers(headers).send().await?;

    let result: serde_json::Value = response.json().await?;
    Ok(result["is_active"] == true)
}
