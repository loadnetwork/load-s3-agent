use crate::core::utils::LCP_API_URL;
use reqwest;
use anyhow::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct LcpBucketResponse {
    pub id: Option<String>,
    pub account_id: Option<String>,
    pub bucket_name: Option<String>,
    pub tx_hash: Option<String>,
    pub block_number: Option<String>,
    pub created_at: Option<String>
}

pub(crate) async fn validate_bucket_ownership(bucket_name: &str, load_acc: &str) -> Result<bool, Error> {
    let url = format!("{LCP_API_URL}/v/{bucket_name}");
    let client = reqwest::Client::new();

    let req = client
        .get(url)
        .header("Authorization", format!("Bearer {}", load_acc))
        .send()
        .await?;

    let req: LcpBucketResponse = req.json().await?;

    if req.bucket_name.is_some() && req.bucket_name.unwrap_or_default() == bucket_name {
        return Ok(true);
    }

    Ok(false)
}