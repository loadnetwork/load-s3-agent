use crate::core::s3::get_bucket_tags;
use anyhow::Error;


pub(crate) async fn validate_bucket_ownership(bucket_name: &str, load_acc: &str) -> Result<bool, Error> {
    let bucket_load_tags = get_bucket_tags(bucket_name).await?;
    return  Ok(bucket_load_tags.contains(&load_acc.to_string()));
}