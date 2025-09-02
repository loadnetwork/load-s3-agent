use crate::core::s3::get_dataitem;
use bundles_rs::bundler::client::BundlerClient;
use bundles_rs::bundler::api::SendTransactionResponse;
use bundles_rs::ans104::data_item::DataItem;
use anyhow::Error;

pub(crate) async fn post_dataitem(id: String) -> Result<SendTransactionResponse, Error> {
    let dataitem = get_dataitem(&id).await?;
    let signed_dataitem = DataItem::from_bytes(&dataitem)?;
    let client = BundlerClient::turbo().build()?;
    let tx = client.send_transaction(signed_dataitem).await?;
    Ok(tx)
}