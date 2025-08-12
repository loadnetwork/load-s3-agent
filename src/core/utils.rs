use anyhow::Error;
use dotenvy::dotenv;
use std::env;

pub(crate) fn get_env_var(key: &str) -> Result<String, Error> {
    dotenv().ok();
    Ok(env::var(key)?)
}
