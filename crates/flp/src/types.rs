use common::projects::INTERNAL_PI_PID;
use serde::{Deserialize, Serialize};

pub const MAX_FACTOR: u32 = 10000;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WalletDelegations {
    pub wallet_to: String,
    pub factor: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DelegationMappingsRow {
    pub wallet_from: String,
    pub wallet_to: String,
    pub factor: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DelegationsRes {
    #[serde(rename = "_key")]
    pub key: Option<String>, // newer version
    pub last_update: Option<u64>,
    pub total_factor: Option<u32>,
    pub wallet: Option<String>,
    pub delegation_prefs: Vec<WalletDelegations>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SetBalancesData {
    pub eoa: String,
    pub amount: String,
    pub ar_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct OwnMintingReport {
    pub distribution_tick: u32,
    pub total_minted: String,
    pub total_inflow: String,
    pub timestamp: u64,
    pub ao_kept: String,
    pub ao_exchanged_for_pi: String,
    pub report_id: Option<String>,
}

impl DelegationsRes {
    pub fn pi_default(address: &str) -> Self {
        let preference = WalletDelegations {
            wallet_to: INTERNAL_PI_PID.to_string(),
            factor: MAX_FACTOR,
        };
        DelegationsRes {
            key: Some(format!("base_{address}")),
            last_update: None,
            total_factor: Some(MAX_FACTOR),
            wallet: Some(address.to_string()),
            delegation_prefs: vec![preference],
        }
    }
}
