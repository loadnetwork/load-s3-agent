use crate::core::{
    ans104::{create_dataitem, reconstruct_dataitem_data},
    utils::{PRESIGNED_URL_EXPIRY, get_env_var},
};
use anyhow::{anyhow, Error};
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

    let s3_config = aws_sdk_s3::config::Builder::from(&config).force_path_style(true).build();
    Ok(Client::from_conf(s3_config))
}

pub async fn store_dataitem(data: Vec<u8>, content_type: &str) -> Result<String, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME").unwrap();
    let s3_dir_name = get_env_var("S3_DIR_NAME").unwrap();
    let s3_dir_name_raw = get_env_var("S3_RAW_DIR_NAME").unwrap();

    let client = s3_client().await?;
    let dataitem = create_dataitem(data.clone(), content_type)?;
    let dataitem_id = dataitem.arweave_id();

    let key_dataitem: String = format!("{s3_dir_name}/{dataitem_id}.ans104");
    let key_raw: String = format!("{s3_dir_name_raw}/{dataitem_id}");

    // store it as ans-104 serialized dataitem
    client
        .put_object()
        .bucket(&s3_bucket_name)
        .key(key_dataitem)
        .body(dataitem.to_bytes()?.into())
        .content_type("application/octet-stream")
        .send()
        .await?;

    // store the dataitem raw body for fast retrievals

    client
        .put_object()
        .bucket(s3_bucket_name)
        .key(key_raw)
        .body(data.into())
        .content_type(content_type)
        .send()
        .await?;

    Ok(dataitem_id)
}

pub async fn store_signed_dataitem(data: Vec<u8>) -> Result<String, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME").unwrap();
    let s3_dir_name = get_env_var("S3_DIR_NAME").unwrap();
    let s3_dir_name_raw = get_env_var("S3_RAW_DIR_NAME").unwrap();

    let client = s3_client().await?;
    let dataitem = reconstruct_dataitem_data(data)?;
    let dataitem_id = dataitem.0.arweave_id();

    let key_dataitem: String = format!("{s3_dir_name}/{dataitem_id}.ans104");
    let key_raw: String = format!("{s3_dir_name_raw}/{dataitem_id}");

    // store it as ans-104 serialized dataitem
    client
        .put_object()
        .bucket(&s3_bucket_name)
        .key(key_dataitem)
        .body(dataitem.0.to_bytes()?.into())
        .content_type("application/octet-stream")
        .send()
        .await?;

    // store the dataitem raw body for fast retrievals

    client
        .put_object()
        .bucket(s3_bucket_name)
        .key(key_raw)
        .body(dataitem.0.data.into())
        .content_type(dataitem.1)
        .send()
        .await?;

    Ok(dataitem_id)
}

pub async fn get_dataitem_url(dataitem_id: &str) -> Result<String, Error> {
    let client = s3_client().await?;
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME")?;
    let s3_dir_name_raw = get_env_var("S3_RAW_DIR_NAME")?;

    let key: String = format!("{s3_dir_name_raw}/{dataitem_id}");

    let presigned_url = client
        .get_object()
        .bucket(s3_bucket_name)
        .key(key)
        .presigned(aws_sdk_s3::presigning::PresigningConfig::expires_in(
            std::time::Duration::from_secs(PRESIGNED_URL_EXPIRY),
        )?)
        .await?;

    Ok(presigned_url.uri().to_string())
}

pub(crate) async fn get_dataitem(dataitem_id: &str) -> Result<Vec<u8>, Error> {
    let client = s3_client().await?;
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME")?;
    let s3_dir_name = get_env_var("S3_DIR_NAME")?;

    let key: String = format!("{s3_dir_name}/{dataitem_id}.ans104");

    let dataitem = client
        .get_object()
        .bucket(s3_bucket_name)
        .key(key)
        .send()
        .await?;

    let data = dataitem.body.collect().await?.into_bytes().to_vec();

    Ok(data)
}

pub async fn get_bucket_stats() -> Result<(u32, u64), Error> {
    let mut continuation_token = None;
    let client: Client = s3_client().await?;
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME")?;
    let s3_dir_name = format!("{}/", get_env_var("S3_DIR_NAME")?);
    let mut total_objects_count: u32 = 0;
    let mut total_objects_size: u64 = 0;
    // with the total_objects_count == 0 loop condition
    // we hack a known condition to simulate do-while.
    // the LCP's bucket will always have more than 1 object.
    while continuation_token.is_some() || total_objects_count == 0 {
        let req = client
            .list_objects_v2()
            .bucket(&s3_bucket_name)
            .delimiter("/")
            .prefix(&s3_dir_name)
            .max_keys(1000)
            .set_continuation_token(continuation_token.clone())
            .send()
            .await?;

        if !req.contents().is_empty() {
            for obj in req.contents() {
                total_objects_count += 1;
                total_objects_size += obj.size().unwrap_or_default() as u64;
            }

            if req.is_truncated().unwrap_or_default() {
                continuation_token =
                    Some(req.next_continuation_token().unwrap_or_default().to_string());
            } else {
                continuation_token = None;
            }
        }
    }

    Ok((total_objects_count, total_objects_size))
}
