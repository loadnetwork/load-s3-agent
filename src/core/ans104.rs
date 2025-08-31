use anyhow::Error;
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
