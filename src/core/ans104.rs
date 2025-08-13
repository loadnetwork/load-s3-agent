use anyhow::{Error, anyhow};
use bundles_rs::{
    ans104::{data_item::DataItem, tags::Tag},
    crypto::arweave::ArweaveSigner,
};

use crate::core::utils::{DATA_PROTOCOL_NAME, get_env_var};

pub(crate) fn create_dataitem(data: Vec<u8>, content_type: &str) -> Result<DataItem, Error> {
    let jwk = get_env_var("UPLOADER_JWK")?;
    let tags =
        vec![Tag::new("Content-Type", content_type), Tag::new("Data-Protocol", DATA_PROTOCOL_NAME)];
    let signer = ArweaveSigner::from_jwk_str(&jwk)?;

    DataItem::build_and_sign(&signer, None, None, tags, data)
}

pub(crate) fn reconstruct_dataitem_data(dataitem: Vec<u8>) -> Result<(Vec<u8>, String), Error> {
    let dataitem = DataItem::from_bytes(&dataitem)?;
    let body = dataitem.data;
    let content_type_tag = dataitem
        .tags
        .iter()
        .find(|tag| tag.name == "Content-Type")
        .ok_or_else(|| anyhow!("Content-Type tag not found"))?;

    Ok((body, content_type_tag.value.clone()))
}
