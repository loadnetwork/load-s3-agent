use anyhow::Error;
use dotenvy::dotenv;
use std::env;

pub(crate) const DATA_PROTOCOL_NAME: &str = "Load-S3";
pub(crate) const DATAITEMS_ADDRESS: &str = "nb0VFuKAdWNvsw-H8bktG89uWaZuC-DFg4b2EcUlFI0";

pub(crate) fn get_env_var(key: &str) -> Result<String, Error> {
    dotenv().ok();
    Ok(env::var(key)?)
}
