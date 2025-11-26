use anyhow::Error;
use crate::constants::ARWEAVE_GATEWAY;

/// downloads an Arweave `txid` data and return Vec<u8> Body
pub fn download_tx_data(txid: &str) -> Result<Vec<u8>, Error> {
    let url = format!("{ARWEAVE_GATEWAY}/{txid}");
    let mut req = ureq::get(url).call()?;
    Ok(req.body_mut().read_to_vec()?)
}