use anyhow::Error;
use bundles_rs::{
    ans104::{data_item::DataItem, tags::Tag},
    crypto::arweave::ArweaveSigner,
};

use crate::core::utils::{STORAGE_PROVIDER_NAME, get_env_var};

const RESERVED_TAGS: [&str; 2] = ["storage-provider", "agent-version"];

pub(crate) fn create_dataitem(
    data: Vec<u8>,
    content_type: &str,
    extra_tags: &[(String, String)],
) -> Result<DataItem, Error> {
    let jwk = get_env_var("UPLOADER_JWK")?;
    let mut tags =
        vec![Tag::new("Content-Type", content_type), Tag::new("Storage-Provider", STORAGE_PROVIDER_NAME), Tag::new("Agent-Version", format!("agent@{}", env!("CARGO_PKG_VERSION")))];
    let signer = ArweaveSigner::from_jwk_str(&jwk)?;

    let mut seen: std::collections::HashSet<String> =
        tags.iter().map(|tag| tag.name.to_lowercase()).collect();

    for (key, value) in extra_tags {
        let key_trimmed = key.trim();
        let value_trimmed = value.trim();
        if key_trimmed.is_empty() || value_trimmed.is_empty() {
            continue;
        }
        // inherts ANS-104 spec tag KV items size limit
        if key_trimmed.len() > 1024 || value_trimmed.len() > 1024 {
            continue;
        }
        let key_lower = key_trimmed.to_lowercase();
        if RESERVED_TAGS.contains(&key_lower.as_str()) {
            continue;
        }
        // the content-type tag is hardcoded at position at index 0
        // if the user provides the mime type in ANS-104 well-known 'Content-Type' tag
        // we use it instead of the http's type=$ field name -- precendency: ANS-104 tag > http field mime type
        // maintaining backward compatibility with versions prior to v0.6.2 (nov 5th 2025)
        if key_lower == "content-type" {
            tags.remove(0);
            tags.push(Tag::new(key_trimmed, value_trimmed));
        }
        if seen.insert(key_lower) {
            tags.push(Tag::new(key_trimmed, value_trimmed));
        }
    }

    DataItem::build_and_sign(&signer, None, None, tags, data)
}

pub(crate) fn reconstruct_dataitem_data(dataitem: Vec<u8>) -> Result<(DataItem, String), Error> {
    let dataitem = DataItem::from_bytes(&dataitem)?;
    let di = dataitem.clone();
    let content_type_tag = di
        .tags
        .iter()
        .find(|tag| tag.name.to_lowercase() == "content-type")
        .map(|tag| tag.value.clone())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok((dataitem, content_type_tag))
}
