use crate::core::utils::get_env_var;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;
use anyhow::Error;

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

// pub async fn store_dataitem(data: Vec<u8>, content_type: &str) {
//     let client = s3_client().await?;

//     let res = client.put_object().bucket(input).key(input).content_type("application/octet-stream").send().await?;
// }