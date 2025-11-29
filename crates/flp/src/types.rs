use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WalletDelegations {
    pub wallet_to: String,
    pub factor: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DelegationsRes {
    #[serde(rename = "_key")]
    pub key: String,
    pub last_update: u64,
    pub delegation_prefs: Vec<WalletDelegations>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetBalancesData {
    pub eoa: String,
    pub amount: u128,
    pub ar_address: String,
}
