use anyhow::Result;
use chrono::Utc;
use common::{
    delegation::{DelegationMappingMeta, DelegationMappingsPage, get_delegation_mappings},
    gateway::get_ar_balance,
    gql::OracleStakers,
    projects::Project,
};
use flp::{
    csv_parser::{parse_delegation_mappings_res, parse_flp_balances_setting_res},
    types::{DelegationsRes, MAX_FACTOR, SetBalancesData},
    wallet::get_wallet_delegations,
};
use futures::{StreamExt, stream};
use rust_decimal::{Decimal, prelude::FromPrimitive};
use serde_json::to_string;
use std::str::FromStr;

use crate::{
    backfill,
    clickhouse::{
        Clickhouse, DelegationMappingRow, FlpPositionRow, OracleSnapshotRow, WalletBalanceRow,
        WalletDelegationRow,
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
        self.spawn_backfill();
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
        self.index_delegation_mappings().await?;
        for ticker in &self.config.tickers {
            self.index_ticker(ticker).await?;
        }
        Ok(())
    }

    fn spawn_backfill(&self) {
        println!("backfill called");
        let clickhouse = self.clickhouse.clone();
        tokio::spawn(async move {
            if let Err(err) = backfill::run(clickhouse).await {
                eprintln!("delegation backfill error: {err:?}");
            }
        });
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

        let pairs: Vec<(SetBalancesData, DelegationsRes, Decimal)> =
            stream::iter(balances.into_iter().map(|entry| async move {
                let delegation = load_delegations(entry.ar_address.clone()).await;
                let ar_balance = load_ar_balance(entry.ar_address.clone()).await;
                (entry, delegation, ar_balance)
            }))
            .buffer_unordered(self.config.concurrency)
            .collect()
            .await;
        println!("ticker {ticker}: delegations {}", pairs.len());

        let mut balance_rows = Vec::with_capacity(pairs.len());
        let mut delegation_rows = Vec::with_capacity(pairs.len());
        let mut position_rows = Vec::new();

        for (entry, delegation, ar_balance) in pairs {
            let Some(amount_dec) = normalize_amount(&entry.amount, &ticker_owned) else {
                continue;
            };
            let amount_str = amount_dec.to_string();
            let ar_balance_str = ar_balance.to_string();
            balance_rows.push(WalletBalanceRow {
                ts: now,
                ticker: ticker_owned.clone(),
                wallet: entry.ar_address.clone(),
                eoa: entry.eoa.clone(),
                amount: amount_str.clone(),
                ar_balance: ar_balance_str.clone(),
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
                    let delegated_ar = delegated_amount(&ar_balance, pref.factor);
                    // if the delegator had interacted with the FLP Bridge, have no more staked LSTs
                    // but still delegating AR, track them
                    if delegated.is_zero() && delegated_ar.is_zero() {
                        continue;
                    }
                    position_rows.push(FlpPositionRow {
                        ts: now,
                        ticker: ticker_owned.clone(),
                        wallet: entry.ar_address.clone(),
                        eoa: entry.eoa.clone(),
                        project: pref.wallet_to,
                        factor: pref.factor,
                        amount: delegated.to_string(),
                        ar_amount: delegated_ar.to_string(),
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

    async fn index_delegation_mappings(&self) -> Result<()> {
        let page = fetch_latest_mapping_page(1).await?;
        let Some(meta) = page.mappings.into_iter().next() else {
            return Ok(());
        };
        if self.clickhouse.has_delegation_mapping(&meta.tx_id).await? {
            return Ok(());
        }
        println!(
            "forward delegation mapping tx {} height {}",
            meta.tx_id, meta.height
        );
        if let Err(err) = self.store_delegation_mapping(&meta).await {
            eprintln!("forward delegation mapping tx {} error {err:?}", meta.tx_id);
        } else {
            println!(
                "forward delegation mapping stored tx {} height {}",
                meta.tx_id, meta.height
            );
        }
        Ok(())
    }

    async fn store_delegation_mapping(&self, meta: &DelegationMappingMeta) -> Result<()> {
        let rows = build_mapping_rows(meta).await?;
        self.clickhouse.insert_delegation_mappings(&rows).await?;
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

async fn load_ar_balance(address: String) -> Decimal {
    match tokio::task::spawn_blocking(move || get_ar_balance(&address)).await {
        Ok(Ok(value)) => Decimal::from_f64(value).unwrap_or(Decimal::ZERO),
        _ => Decimal::ZERO,
    }
}

async fn fetch_latest_mapping_page(limit: u32) -> Result<DelegationMappingsPage> {
    Ok(tokio::task::spawn_blocking(move || get_delegation_mappings(Some(limit), None)).await??)
}

async fn build_mapping_rows(meta: &DelegationMappingMeta) -> Result<Vec<DelegationMappingRow>> {
    let tx_id = meta.tx_id.clone();
    let height = meta.height;
    let csv_rows = tokio::task::spawn_blocking({
        let fetch_id = tx_id.clone();
        move || parse_delegation_mappings_res(&fetch_id)
    })
    .await??;
    let ts = Utc::now();
    Ok(csv_rows
        .into_iter()
        .map(|row| DelegationMappingRow {
            ts: ts.clone(),
            height,
            tx_id: tx_id.clone(),
            wallet_from: row.wallet_from,
            wallet_to: row.wallet_to,
            factor: row.factor,
        })
        .collect())
}
