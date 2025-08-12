use crate::core::{ans104::create_dataitem, utils::get_env_var};
use anyhow::Error;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;

/// Initialize the ~s3@1.0 device connection using the aws s3 sdk.
async fn s3_client() -> Result<Client, Error> {
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(get_env_var("AWS_ENDPOINT_URL").unwrap())
        .region(Region::new(get_env_var("AWS_REGION").unwrap()))
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            get_env_var("AWS_ACCESS_KEY_ID").unwrap(),
            get_env_var("AWS_SECRET_ACCESS_KEY").unwrap(),
            None,
            None,
            "custom",
        ))
        .load()
        .await;
    Ok(Client::new(&config))
}

pub async fn store_dataitem(data: Vec<u8>, content_type: &str) -> Result<String, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME").unwrap();
    let s3_dir_name = get_env_var("S3_DIR_NAME").unwrap();

    let client = s3_client().await?;
    let dataitem = create_dataitem(data, content_type)?;
    let dataitem_id = dataitem.arweave_id();

    let key: String = format!("{s3_bucket_name}/{s3_dir_name}/{dataitem_id}.ans104");

    client
        .put_object()
        .bucket(s3_bucket_name)
        .key(key)
        .body(dataitem.to_bytes()?.into())
        .content_type("application/octet-stream")
        .send()
        .await?;

    Ok(dataitem_id)
}
