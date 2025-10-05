use crate::core::s3::get_dataitem;
use anyhow::Error;
use bundles_rs::{
    ans104::data_item::DataItem,
    bundler::{BundlerClient, SendTransactionResponse},
};

pub(crate) async fn post_dataitem(id: String) -> Result<SendTransactionResponse, Error> {
    let dataitem = get_dataitem(&id).await?;
    let signed_dataitem = DataItem::from_bytes(&dataitem)?;
    let client = BundlerClient::turbo().build()?;
    let tx = client.send_transaction(signed_dataitem).await?;
    Ok(tx)
}
