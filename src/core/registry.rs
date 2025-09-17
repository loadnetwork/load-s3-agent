use crate::core::utils::get_env_var;
use anyhow::Error;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct RegistryEntry {
    pub bucket_name: String,
    pub dataitem_id: String,
    pub dataitem_name: String,
}

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Registry {
    pub data: Vec<RegistryEntry>,
}

fn load_registry(path: &str) -> Result<Registry, Error> {
    if Path::new(path).exists() {
        let content = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(Registry::default())
    }
}

fn save_registry(path: &str, registry: &Registry) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(registry)?;
    fs::write(path, json)?;
    Ok(())
}

pub(crate) fn set_dataitem_name(
    bucket_name: &str,
    dataitem_id: &str,
    dataitem_name: &str,
) -> Result<bool, Error> {
    let registry_path = get_env_var("S3_AGENT_REGISTRY_FILE_PATH")?;

    if let Some(parent) = Path::new(&registry_path).parent() {
        fs::create_dir_all(parent)?;
    }

    let mut registry = load_registry(&registry_path)?;
    registry.data.push(RegistryEntry {
        bucket_name: bucket_name.to_string(),
        dataitem_id: dataitem_id.to_string(),
        dataitem_name: dataitem_name.to_string(),
    });

    save_registry(&registry_path, &registry)?;

    Ok(true)
}

pub(crate) fn get_bucket_registry(bucket_name: &str) -> Result<Vec<RegistryEntry>, Error> {
    let registry_path = get_env_var("S3_AGENT_REGISTRY_FILE_PATH")?;

    let registry = load_registry(&registry_path)?;
    let dataitems: Vec<RegistryEntry> =
        registry.data.iter().filter(|di| di.bucket_name == bucket_name).cloned().collect();

    Ok(dataitems)
}
