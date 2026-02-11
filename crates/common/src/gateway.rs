use crate::constants::arweave_gateway;
use anyhow::Error;

/// downloads an Arweave `txid` data and return Vec<u8> Body
pub fn download_tx_data(txid: &str) -> Result<Vec<u8>, Error> {
    let url = format!("{}/{txid}", arweave_gateway());
    let mut req = ureq::get(url).call()?;
    Ok(req.body_mut().read_to_vec()?)
}

/// gets the AR balance of a given Arweave address
pub fn get_ar_balance(address: &str) -> Result<f64, Error> {
    let url = format!("{}/wallet/{address}/balance", arweave_gateway());
    let mut req = ureq::get(url).call()?;
    let winston = req.body_mut().read_to_string()?;
    let winston = winston.parse::<f64>()?;
    Ok(winston * 1e-12)
}
