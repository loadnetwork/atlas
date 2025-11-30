use anyhow::Result;
use chrono::Utc;
use common::{gql::OracleStakers, projects::Project};
use flp::{
    set_balances::parse_flp_balances_setting_res,
    types::{DelegationsRes, MAX_FACTOR, SetBalancesData},
    wallet::get_wallet_delegations,
};
use futures::{StreamExt, stream};
use rust_decimal::Decimal;
use serde_json::to_string;
use std::str::FromStr;

use crate::{
    clickhouse::{
        Clickhouse, FlpPositionRow, OracleSnapshotRow, WalletBalanceRow, WalletDelegationRow,
    },
    config::Config,
};

pub struct Indexer {
    config: Config,
    clickhouse: Clickhouse,
}

impl Indexer {
    pub fn new(config: Config, clickhouse: Clickhouse) -> Self {
        Indexer { config, clickhouse }
    }

    pub async fn run(&self) -> Result<()> {
        self.clickhouse.ensure().await?;
        println!("indexer ready with tickers {:?}", self.config.tickers);
        self.run_once().await?;
        let mut interval = tokio::time::interval(self.config.interval);
        loop {
            println!("waiting {:?}", self.config.interval);
            interval.tick().await;
            println!("starting new cycle");
            if let Err(err) = self.run_once().await {
                eprintln!("index cycle error: {err:?}");
            }
        }
    }

    async fn run_once(&self) -> Result<()> {
        for ticker in &self.config.tickers {
            self.index_ticker(ticker).await?;
        }
        Ok(())
    }

    async fn index_ticker(&self, ticker: &str) -> Result<()> {
        let now = Utc::now();
        let ticker_owned = ticker.to_string();
        let (tx_id, balances) = load_balances(ticker_owned.clone()).await?;
        if self.clickhouse.has_oracle(&ticker_owned, &tx_id).await? {
            println!("ticker {ticker}: tx {tx_id} already processed, skipping");
            return Ok(());
        }
        println!("ticker {ticker}: loading balances");
        println!("ticker {ticker}: balances {}", balances.len());
        self.clickhouse
            .insert_oracles(&[OracleSnapshotRow {
                ts: now,
                ticker: ticker_owned.clone(),
                tx_id: tx_id.clone(),
            }])
            .await?;

        let pairs: Vec<(SetBalancesData, DelegationsRes)> =
            stream::iter(balances.into_iter().map(|entry| async move {
                let delegation = load_delegations(entry.ar_address.clone()).await;
                (entry, delegation)
            }))
            .buffer_unordered(self.config.concurrency)
            .collect()
            .await;
        println!("ticker {ticker}: delegations {}", pairs.len());

        let mut balance_rows = Vec::with_capacity(pairs.len());
        let mut delegation_rows = Vec::with_capacity(pairs.len());
        let mut position_rows = Vec::new();

        for (entry, delegation) in pairs {
            let Some(amount_dec) = normalize_amount(&entry.amount, &ticker_owned) else {
                continue;
            };
            let amount_str = amount_dec.to_string();
            println!(
                "wallet {} ticker {} balance {}",
                entry.ar_address, ticker_owned, amount_str
            );
            balance_rows.push(WalletBalanceRow {
                ts: now,
                ticker: ticker_owned.clone(),
                wallet: entry.ar_address.clone(),
                amount: amount_str.clone(),
                tx_id: tx_id.clone(),
            });
            delegation_rows.push(WalletDelegationRow {
                ts: now,
                wallet: entry.ar_address.clone(),
                payload: to_string(&delegation)?,
            });
            for pref in delegation.delegation_prefs {
                if Project::is_flp_project(&pref.wallet_to) {
                    let delegated = delegated_amount(&amount_dec, pref.factor);
                    if delegated.is_zero() {
                        continue;
                    }
                    println!(
                        "wallet {} ticker {} project {} factor {} delegated {}",
                        entry.ar_address, ticker_owned, pref.wallet_to, pref.factor, delegated
                    );
                    position_rows.push(FlpPositionRow {
                        ts: now,
                        ticker: ticker_owned.clone(),
                        wallet: entry.ar_address.clone(),
                        project: pref.wallet_to,
                        factor: pref.factor,
                        amount: delegated.to_string(),
                    });
                }
            }
        }

        self.clickhouse.insert_balances(&balance_rows).await?;
        self.clickhouse.insert_delegations(&delegation_rows).await?;
        self.clickhouse.insert_positions(&position_rows).await?;
        println!(
            "ticker {ticker}: stored balances {} delegations {} positions {}",
            balance_rows.len(),
            delegation_rows.len(),
            position_rows.len()
        );
        Ok(())
    }
}

fn normalize_amount(amount: &str, ticker: &str) -> Option<Decimal> {
    let amt = Decimal::from_str(amount).ok()?;
    Some((amt / ticker_scale(ticker)).normalize())
}

// all 3 oracles tokens are 18 decimals
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

async fn load_balances(ticker: String) -> Result<(String, Vec<SetBalancesData>)> {
    Ok(
        tokio::task::spawn_blocking(move || -> Result<(String, Vec<SetBalancesData>)> {
            let oracle = OracleStakers::new(&ticker).build()?.send()?;
            let tx_id = oracle.clone().last_update()?;
            let data = parse_flp_balances_setting_res(&tx_id)?;
            Ok((tx_id, data))
        })
        .await??,
    )
}

async fn load_delegations(address: String) -> DelegationsRes {
    let fallback = address.clone();
    match tokio::task::spawn_blocking(move || get_wallet_delegations(&address)).await {
        Ok(Ok(data)) => data,
        _ => DelegationsRes::pi_default(&fallback),
    }
}
