use crate::core::{
    ans104::{create_dataitem, reconstruct_dataitem_data},
    lcp::validate_bucket_ownership,
    metadata::index_dataitem,
    registry::set_dataitem_name,
    utils::{PRESIGNED_URL_EXPIRY, get_env_var},
};
use anyhow::{Error, anyhow};
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;

#[derive(Debug, Clone)]
pub(crate) struct AgentConfig {
    pub endpoint_url: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub s3_bucket_name: String,
    pub s3_dir_name: String,
    pub s3_raw_dir_name: String,
}

impl AgentConfig {
    pub fn load() -> AgentConfig {
        AgentConfig {
            endpoint_url: get_env_var("AWS_ENDPOINT_URL").unwrap(),
            region: get_env_var("AWS_REGION").unwrap(),
            access_key_id: get_env_var("AWS_ACCESS_KEY_ID").unwrap(),
            secret_access_key: get_env_var("AWS_SECRET_ACCESS_KEY").unwrap(),
            s3_bucket_name: get_env_var("S3_BUCKET_NAME").unwrap(),
            s3_dir_name: get_env_var("S3_DIR_NAME").unwrap(),
            s3_raw_dir_name: get_env_var("S3_RAW_DIR_NAME").unwrap(),
        }
    }
}

/// Initialize the ~s3@1.0 device connection using the aws s3 sdk.
async fn s3_client() -> Result<Client, Error> {
    let agent_config = AgentConfig::load();
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(agent_config.endpoint_url)
        .region(Region::new(agent_config.region))
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            &agent_config.access_key_id,
            &agent_config.secret_access_key,
            None,
            None,
            "custom",
        ))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config).force_path_style(true).build();
    Ok(Client::from_conf(s3_config))
}

pub async fn store_dataitem(
    data: Vec<u8>,
    content_type: &str,
    extra_tags: &[(String, String)],
) -> Result<String, Error> {
    let agent_config = AgentConfig::load();
    let client = s3_client().await?;
    let dataitem = create_dataitem(data.clone(), content_type, extra_tags)?;
    let tags_for_index: Vec<(String, String)> =

        dataitem.tags.iter().map(|tag| (tag.name.clone(), tag.value.clone())).collect();
    let dataitem_id = dataitem.arweave_id();

    let key_dataitem: String = format!("{}/{dataitem_id}.ans104", agent_config.s3_dir_name);
    let key_raw: String = format!("{}/{dataitem_id}", agent_config.s3_raw_dir_name);

    // store it as ans-104 serialized dataitem
    client
        .put_object()
        .bucket(&agent_config.s3_bucket_name)
        .key(key_dataitem)
        .body(dataitem.to_bytes()?.into())
        .content_type("application/octet-stream")
        .send()
        .await?;

    // store the dataitem raw body for fast retrievals

    client
        .put_object()
        .bucket(agent_config.s3_bucket_name)
        .key(key_raw)
        .body(data.into())
        .content_type(content_type)
        .send()
        .await?;

    println!("INDEX DATA: {:?} {:?} {:?}", &dataitem_id, &content_type, &tags_for_index);
    index_dataitem(&dataitem_id, content_type, &tags_for_index).await.unwrap();

    Ok(dataitem_id)
}

pub async fn store_signed_dataitem(data: Vec<u8>) -> Result<String, Error> {
    let agent_config = AgentConfig::load();
    let client = s3_client().await?;
    let (dataitem, content_type) = reconstruct_dataitem_data(data)?;
    let dataitem_id = dataitem.arweave_id();
    let tags_for_index: Vec<(String, String)> =
        dataitem.tags.iter().map(|tag| (tag.name.clone(), tag.value.clone())).collect();

    let key_dataitem: String = format!("{}/{dataitem_id}.ans104", agent_config.s3_dir_name);
    let key_raw: String = format!("{}/{dataitem_id}", agent_config.s3_raw_dir_name);

    // store it as ans-104 serialized dataitem
    client
        .put_object()
        .bucket(&agent_config.s3_bucket_name)
        .key(key_dataitem)
        .body(dataitem.to_bytes()?.into())
        .content_type("application/octet-stream")
        .send()
        .await?;

    // store the dataitem raw body for fast retrievals

    client
        .put_object()
        .bucket(agent_config.s3_bucket_name)
        .key(key_raw)
        .body(dataitem.data.clone().into())
        .content_type(content_type.clone())
        .send()
        .await?;

    index_dataitem(&dataitem_id, &content_type, &tags_for_index).await?;

    Ok(dataitem_id)
}

pub async fn get_dataitem_url(dataitem_id: &str) -> Result<String, Error> {
    let agent_config = AgentConfig::load();
    let client = s3_client().await?;
    // i think we should default to signed dataitems: agent_config.s3_dir_name
    // TODO: check which dependencies rely on dataitem's data expected response
    let key: String = format!("{}/{dataitem_id}", agent_config.s3_raw_dir_name);

    let presigned_url = client
        .get_object()
        .bucket(agent_config.s3_bucket_name)
        .key(key)
        .presigned(aws_sdk_s3::presigning::PresigningConfig::expires_in(
            std::time::Duration::from_secs(PRESIGNED_URL_EXPIRY),
        )?)
        .await?;

    Ok(presigned_url.uri().to_string())
}

pub(crate) async fn get_dataitem(dataitem_id: &str) -> Result<Vec<u8>, Error> {
    let agent_config = AgentConfig::load();
    let client = s3_client().await?;

    let key: String = format!("{}/{dataitem_id}.ans104", agent_config.s3_dir_name);

    let dataitem = client.get_object().bucket(agent_config.s3_bucket_name).key(key).send().await?;

    let data = dataitem.body.collect().await?.into_bytes().to_vec();

    Ok(data)
}

pub async fn get_bucket_stats() -> Result<(u32, u64), Error> {
    let agent_config = AgentConfig::load();
    let mut continuation_token = None;
    let client: Client = s3_client().await?;
    let mut total_objects_count: u32 = 0;
    let mut total_objects_size: u64 = 0;
    // with the total_objects_count == 0 loop condition
    // we hack a known condition to simulate do-while.
    // the LCP's bucket will always have more than 1 object.
    while continuation_token.is_some() || total_objects_count == 0 {
        let req = client
            .list_objects_v2()
            .bucket(&agent_config.s3_bucket_name)
            .delimiter("/")
            .prefix(&agent_config.s3_dir_name)
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

pub async fn store_lcp_priv_bucket_dataitem(
    data: Vec<u8>,
    content_type: &str,
    bucket_name: &str,
    folder_name: &str,
    load_acc: &str,
    dataitem_name: &str,
    is_signed: bool,
) -> Result<String, Error> {
    if !validate_bucket_ownership(bucket_name, load_acc).await? {
        return Err(anyhow!("invalid load_acc api key"));
    }

    let client = s3_client().await?;

    let dataitem = if is_signed {
        reconstruct_dataitem_data(data)?.0
    } else {
        create_dataitem(data.clone(), content_type, &[])?
    };

    let dataitem_id = dataitem.arweave_id();

    let key_dataitem = if !folder_name.is_empty() {
        format!("{folder_name}/{dataitem_id}.ans104")
    } else {
        format!("{dataitem_id}.ans104")
    };

    // store it as ans-104 serialized dataitem
    client
        .put_object()
        .bucket(bucket_name)
        .key(&key_dataitem)
        .body(dataitem.to_bytes()?.into())
        // set name even if its empty
        .tagging(format!("dataitem-name={dataitem_name}"))
        .content_type("application/octet-stream")
        .send()
        .await?;

    // register the dataitem name if provided
    if !dataitem_name.is_empty() {
        set_dataitem_name(bucket_name, &key_dataitem, dataitem_name)?;
    }

    Ok(dataitem_id)
}

pub(crate) async fn get_bucket_tags(bucket_name: &str) -> Result<Vec<String>, Error> {
    let client = s3_client().await?;

    let req = client.get_bucket_tagging().bucket(bucket_name).send().await?;
    let tags: Vec<(String, String)> =
        req.tag_set.iter().map(|tag| (tag.key.to_string(), tag.value.to_string())).collect();
    let load_acc_tags: Vec<String> =
        tags.iter().filter(|tag| tag.1.starts_with("load_acc_")).map(|tag| tag.1.clone()).collect();
    Ok(load_acc_tags)
}
