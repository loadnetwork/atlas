use crate::types::DelegationsRes;
use anyhow::Error;
use common::gateway::download_tx_data;
use common::gql::{get_user_delegation_txid, get_user_last_delegation_txid};

/// retrieves wallet delegation preferences by making two queries:
/// 1- gets the last delegation message ID (msg sent from user addr to DELEGATION_PID)
/// 2- extracts the actual delegation data from its `Pushed-For` tag
/// (msg sent from AO_AUTHORITY to user address with From-Process & Pushed-For tags)
pub fn get_wallet_delegations(address: &str) -> Result<DelegationsRes, Error> {
    let last_delegation_txid = get_user_last_delegation_txid(address)?;
    let delegation_txid = get_user_delegation_txid(&last_delegation_txid)?;
    let delegation_data = download_tx_data(&delegation_txid)?;
    let res: DelegationsRes = serde_json::from_slice(&delegation_data)?;
    Ok(res)
}

#[cfg(test)]
mod tests {
    use crate::wallet::get_wallet_delegations;

    #[test]
    fn get_wallet_delegations_test() {
        let address = "vZY2XY1RD9HIfWi8ift-1_DnHLDadZMWrufSh-_rKF0";
        let req = get_wallet_delegations(address).unwrap();
        println!("wallet delegations: {:?}", req);
        assert!(req.key == format!("base_{address}"));
    }
}
