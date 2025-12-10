use anyhow::{anyhow, Result};
use common::gql::OracleStakers;
use flp::{
    csv_parser::parse_flp_balances_setting_res,
    types::{DelegationsRes, MAX_FACTOR},
    wallet::get_wallet_delegations,
};
use rust_decimal::Decimal;
use serde::Serialize;
use std::{env, str::FromStr};

#[derive(Serialize)]
struct DelegationDebugRow {
    wallet: String,
    eoa: String,
    factor: u32,
    wallet_amount: String,
    delegated_amount: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let ticker = args
        .next()
        .ok_or_else(|| anyhow!("usage: debug_flp_cycle <ticker> <project_id> [oracle_tx_id]"))?;
    let project = args
        .next()
        .ok_or_else(|| anyhow!("usage: debug_flp_cycle <ticker> <project_id> [oracle_tx_id]"))?;
    let tx_id = if let Some(tx) = args.next() {
        tx
    } else {
        latest_oracle_tx(&ticker).await?
    };

    let balances = parse_flp_balances_setting_res(&tx_id)?;
    eprintln!(
        "ticker {ticker} project {project} tx {tx_id}: balances {}",
        balances.len()
    );
    let mut out = Vec::new();
    for entry in balances {
        let Some(amount_dec) = normalize_amount(&entry.amount, &ticker) else {
            eprintln!("wallet {} skipped: invalid amount {}", entry.ar_address, entry.amount);
            continue;
        };
        eprintln!(
            "wallet {} eoa {} amount {}",
            entry.ar_address, entry.eoa, amount_dec
        );
        let delegation = load_delegations(entry.ar_address.clone()).await;
        for pref in delegation.delegation_prefs {
            if pref.wallet_to != project {
                eprintln!(
                    "wallet {} pref project {} ignored",
                    entry.ar_address, pref.wallet_to
                );
                continue;
            }
            let delegated = delegated_amount(&amount_dec, pref.factor);
            if delegated.is_zero() {
                eprintln!(
                    "wallet {} pref factor {} delegated zero",
                    entry.ar_address, pref.factor
                );
                continue;
            }
            out.push(DelegationDebugRow {
                wallet: entry.ar_address.clone(),
                eoa: entry.eoa.clone(),
                factor: pref.factor,
                wallet_amount: amount_dec.to_string(),
                delegated_amount: delegated.to_string(),
            });
            eprintln!(
                "wallet {} matched factor {} delegated {}",
                entry.ar_address, pref.factor, delegated
            );
        }
    }
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

async fn latest_oracle_tx(ticker: &str) -> Result<String> {
    let ticker = ticker.to_string();
    tokio::task::spawn_blocking(move || {
        let oracle = OracleStakers::new(&ticker).build()?.send()?;
        oracle.clone().last_update()
    })
    .await?
    .map_err(Into::into)
}

async fn load_delegations(address: String) -> DelegationsRes {
    let fallback = address.clone();
    match tokio::task::spawn_blocking(move || get_wallet_delegations(&address)).await {
        Ok(Ok(data)) => data,
        _ => DelegationsRes::pi_default(&fallback),
    }
}

fn normalize_amount(amount: &str, ticker: &str) -> Option<Decimal> {
    let amt = Decimal::from_str(amount).ok()?;
    Some((amt / ticker_scale(ticker)).normalize())
}

fn ticker_scale(ticker: &str) -> Decimal {
    let key = ticker.to_ascii_lowercase();
    match key.as_str() {
        "usds" | "dai" | "steth" => Decimal::from_str("1000000000000000000").unwrap(),
        _ => Decimal::ONE,
    }
}

fn delegated_amount(amount: &Decimal, factor: u32) -> Decimal {
    (amount * Decimal::from(factor) / Decimal::from(MAX_FACTOR)).normalize()
}
