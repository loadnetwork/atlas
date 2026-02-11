use crate::env::get_env_var;
use serde::Deserialize;
use std::{fs, io::ErrorKind, sync::OnceLock};

// FLP system yield oracle processes -- legacy
pub const USDS_ORACLE_PID: &str = "qjOMZnan8Vo2gaLaOF1FXbFXOQOn_5sKbYspNSVRyNY";
pub const USDS_STAKING_ADDRESS: &str = "0x7cd01d5cad4ba0caeba02583a5c61d35b23e08eb";
pub const STETH_ORACLE_PID: &str = "wJV8FMkpoeLsTjJ6O7YZEuQgMqj-sDjPHhTeA73RsCc";
pub const STETH_STAKING_ADDRESS: &str = "0xfe08d40eee53d64936d3128838867c867602665c";
pub const DAI_ORACLE_PID: &str = "5q8vpzC5QAKOAJFM26MAKfZw1gwtw7WA_J2861ZiKhI";
pub const DAI_STAKING_ADDRESS: &str = "0x6a1b588b0684dace1f53c5820111f400b3dbfebf";
// ao protocol -- legacy
pub const AO_AUTHORITY: &str = "fcoN_xJeisVsPXA-trzVAuIiqO3ydLQxM-L4XbrQKzY";
pub const DELEGATION_PID: &str = "cuxSKjGJ-WDB9PzSkVkVVrIBSh3DrYHYz44usQOj5yE";
pub const AO_TOKEN_PROCESS: &str = "0syT13r0s0tgPmIed95bJnuSqaD29HQNN8D3ElLSrsc";
pub const AO_TOKEN_START: u32 = 1_606_012;
pub const PI_TOKEN_PROCESS: &str = "4hXj_E-5fAKmo4E8KjgQvuDJKAFk9P2grhycVmISDLs";
pub const PI_TOKEN_START: u32 = 1_638_421;
// ao mainnet data protocols
// the mainnet have 2 type of tags for mainnet txs,
// type A follows lower-case tags key format
// type B follows Header-Case tags key format
pub const DATA_PROTOCOL_A_START: u32 = 1_594_020; // Jan 22 2025
pub const DATA_PROTOCOL_B_START: u32 = 1_616_999; // Feb 25 2025
// endpoints
const DEFAULT_ARWEAVE_GATEWAY: &str = "https://arweave.net";

pub fn arweave_gateway() -> &'static str {
    static GATEWAY: OnceLock<String> = OnceLock::new();
    GATEWAY.get_or_init(load_arweave_gateway).as_str()
}

fn load_arweave_gateway() -> String {
    let path = get_env_var("ATLAS_CONFIG").unwrap_or_else(|_| "atlas.toml".into());
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return DEFAULT_ARWEAVE_GATEWAY.to_string();
        }
        Err(err) => {
            eprintln!("failed to read atlas config {path}: {err}");
            return DEFAULT_ARWEAVE_GATEWAY.to_string();
        }
    };
    let config = match toml::from_str::<AtlasConfig>(&contents) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("failed to parse atlas config {path}: {err}");
            return DEFAULT_ARWEAVE_GATEWAY.to_string();
        }
    };
    config
        .primary_arweave_gateway
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_ARWEAVE_GATEWAY.to_string())
}

#[derive(Deserialize, Default)]
struct AtlasConfig {
    #[serde(rename = "PRIMARY_ARWEAVE_GATEWAY", alias = "primary_arweave_gateway")]
    primary_arweave_gateway: Option<String>,
}
