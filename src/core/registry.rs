use crate::core::utils::get_env_var;
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct RegistryEntry {
    pub dataitem_id: String,
    pub dataitem_name: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct BucketRegistry {
    pub bucket_name: String,
    pub data: Vec<RegistryEntry>,
}

fn get_bucket_file_path(bucket_name: &str) -> Result<PathBuf, Error> {
    let registry_dir = get_env_var("S3_AGENT_REGISTRY_DIR_PATH")?;

    // Sanitize bucket name for filesystem
    let safe_bucket_name = bucket_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");

    Ok(Path::new(&registry_dir).join(format!("{safe_bucket_name}.json")))
}

fn load_bucket_registry(bucket_name: &str) -> Result<BucketRegistry, Error> {
    let file_path = get_bucket_file_path(bucket_name)?;

    if file_path.exists() {
        let content = fs::read_to_string(&file_path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(BucketRegistry { bucket_name: bucket_name.to_string(), data: Vec::new() })
    }
}

fn save_bucket_registry(registry: &BucketRegistry) -> Result<(), Error> {
    let file_path = get_bucket_file_path(&registry.bucket_name)?;

    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(registry)?;
    fs::write(&file_path, json)?;
    Ok(())
}

pub(crate) fn set_dataitem_name(
    bucket_name: &str,
    dataitem_id: &str,
    dataitem_name: &str,
) -> Result<bool, Error> {
    let mut registry = load_bucket_registry(bucket_name)?;

    // check if dataitem entry already exists and update, or add new
    if let Some(existing) = registry.data.iter_mut().find(|entry| entry.dataitem_id == dataitem_id)
    {
        existing.dataitem_name = dataitem_name.to_string();
    } else {
        registry.data.push(RegistryEntry {
            dataitem_id: dataitem_id.to_string(),
            dataitem_name: dataitem_name.to_string(),
        });
    }

    save_bucket_registry(&registry)?;
    Ok(true)
}

pub(crate) fn get_bucket_registry(bucket_name: &str) -> Result<Vec<RegistryEntry>, Error> {
    let registry = load_bucket_registry(bucket_name)?;
    Ok(registry.data)
}
