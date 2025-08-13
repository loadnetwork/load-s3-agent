use anyhow::Error;
use dotenvy::dotenv;
use std::env;

pub(crate) const DATA_PROTOCOL_NAME: &str = "Load-S3";
pub(crate) const DATAITEMS_ADDRESS: &str = "2BBwe2pSXn_Tp-q_mHry0Obp88dc7L-eDIWx0_BUfD0";
pub(crate) const PRESIGNED_URL_EXPIRY: u64 = 3600;
pub const OBJECT_SIZE_LIMIT: usize = 1000 * 1024 * 1024; // 1GB

pub(crate) fn get_env_var(key: &str) -> Result<String, Error> {
    dotenv().ok();
    Ok(env::var(key)?)
}
