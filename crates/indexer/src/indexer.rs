use anyhow::Result;
use chrono::Utc;
use common::{
    ao_token::{
        AoTokenMessageMeta, AoTokenMessagesPage, AoTokenQuery, scan_arweave_block_for_token_msgs,
    },
    constants::{
        AO_TOKEN_PROCESS, AO_TOKEN_START, DATA_PROTOCOL_A_START, DATA_PROTOCOL_B_START,
        PI_TOKEN_PROCESS, PI_TOKEN_START,
    },
    delegation::{DelegationMappingMeta, DelegationMappingsPage, get_delegation_mappings},
    gateway::get_ar_balance,
    gql::OracleStakers,
    mainnet::{
        DataProtocol, MainnetBlockMessagesMeta, MainnetBlockMessagesPage, get_network_height,
        scan_arweave_block_for_msgs,
    },
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
use tokio::{
    runtime::Handle,
    time::{Duration, sleep},
};

use crate::{
    clickhouse::{
        AoTokenBlockStateRow, AoTokenMessageRow, AoTokenMessageTagRow, AtlasExplorerRow,
        Clickhouse, DelegationMappingRow, FlpPositionRow, MainnetBlockStateRow, MainnetExplorerRow,
        MainnetMessageRow, MainnetMessageTagRow, OracleSnapshotRow, WalletBalanceRow,
        WalletDelegationRow,
    },
    config::Config,
};
// use explorer;

const ARWEAVE_TIP_SAFE_GAP: u64 = 3;

#[derive(Clone, Copy)]
struct TokenConfig {
    label: &'static str,
    process_id: &'static str,
    start_height: u32,
}

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
        // self.reindex_mainnet_gap(1_821_500).await?;
        self.spawn_explorer_bridge().await?;
        self.spawn_mainnet_indexer().await?;
        self.spawn_ao_token_indexer().await?;
        self.rebuild_mainnet_explorer().await?;
        self.spawn_mainnet_explorer_tail().await?;
        // self.spawn_backfill();
        println!("indexer ready with tickers {:?}", self.config.tickers);
        if let Err(err) = self.run_once().await {
            eprintln!("index cycle error: {err:?}");
        }
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
        if let Err(err) = self.index_delegation_mappings().await {
            eprintln!("delegation mapping error: {err:?}");
        }
        for ticker in &self.config.tickers {
            if let Err(err) = self.index_ticker(ticker).await {
                eprintln!("ticker {ticker} error: {err:?}");
            }
        }
        Ok(())
    }

    async fn spawn_explorer_bridge(&self) -> Result<()> {
        let start = self
            .clickhouse
            .latest_explorer_stats()
            .await?
            .unwrap_or_else(|| explorer::update_stats_gap::LATEST_AGG_STATS_SET.clone());
        let clickhouse = self.clickhouse.clone();
        let handle = Handle::current();
        std::thread::spawn(move || {
            if let Err(err) = explorer::run_stats_indexer_from(start, |stats| {
                let row = match AtlasExplorerRow::from_block_stats(stats) {
                    Some(row) => row,
                    None => return Ok(()),
                };
                let rows = [row];
                handle.block_on(async { clickhouse.insert_explorer_stats(&rows).await })
            }) {
                eprintln!("atlas explorer indexer error: {err:?}");
            }
        });
        Ok(())
    }

    // async fn reindex_mainnet_gap(&self, start: u32) -> Result<()> {
    //     for protocol in [DataProtocol::A, DataProtocol::B] {
    //         run_mainnet_gap_worker(self.clickhouse.clone(), protocol, start).await?;
    //     }
    //     Ok(())
    // }

    async fn spawn_mainnet_indexer(&self) -> Result<()> {
        for (protocol, start) in [
            (DataProtocol::A, DATA_PROTOCOL_A_START),
            (DataProtocol::B, DATA_PROTOCOL_B_START),
        ] {
            let clickhouse = self.clickhouse.clone();
            tokio::spawn(async move {
                if let Err(err) = run_mainnet_worker(clickhouse, protocol, start).await {
                    eprintln!(
                        "mainnet indexer error protocol={} start={} err={err:?}",
                        protocol_label(protocol),
                        start
                    );
                }
            });
        }
        Ok(())
    }

    async fn spawn_ao_token_indexer(&self) -> Result<()> {
        let tokens = [
            TokenConfig {
                label: "ao",
                process_id: AO_TOKEN_PROCESS,
                start_height: AO_TOKEN_START,
            },
            TokenConfig {
                label: "pi",
                process_id: PI_TOKEN_PROCESS,
                start_height: PI_TOKEN_START,
            },
        ];
        for token in tokens {
            let clickhouse = self.clickhouse.clone();
            tokio::spawn(async move {
                if let Err(err) = run_token_worker(clickhouse, token).await {
                    eprintln!(
                        "token indexer error token={} start={} err={err:?}",
                        token.label, token.start_height
                    );
                }
            });
        }
        Ok(())
    }

    async fn rebuild_mainnet_explorer(&self) -> Result<()> {
        println!("rebuilding ao mainnet explorer table from scratch");
        self.clickhouse.truncate_mainnet_explorer().await?;
        let mut last_height: u32 = 0;
        let mut tx_roll: u64 = 0;
        let mut proc_roll: u64 = 0;
        let mut mod_roll: u64 = 0;
        loop {
            let metrics = self
                .clickhouse
                .fetch_mainnet_block_metrics(last_height, 512)
                .await?;
            if metrics.is_empty() {
                break;
            }
            let mut rows = Vec::with_capacity(metrics.len());
            for metric in metrics {
                last_height = metric.height;
                tx_roll += metric.tx_count;
                proc_roll += metric.new_process_count;
                mod_roll += metric.new_module_count;
                rows.push(MainnetExplorerRow {
                    ts: metric.ts,
                    height: metric.height as u64,
                    tx_count: metric.tx_count,
                    eval_count: metric.eval_count,
                    transfer_count: metric.transfer_count,
                    new_process_count: metric.new_process_count,
                    new_module_count: metric.new_module_count,
                    active_users: metric.active_users,
                    active_processes: metric.active_processes,
                    tx_count_rolling: tx_roll,
                    processes_rolling: proc_roll,
                    modules_rolling: mod_roll,
                });
            }
            self.clickhouse.insert_mainnet_explorer_rows(&rows).await?;
            println!("mainnet explorer indexed up to height {last_height}");
        }
        println!("ao mainnet explorer rebuild complete");
        Ok(())
    }

    async fn spawn_mainnet_explorer_tail(&self) -> Result<()> {
        let clickhouse = self.clickhouse.clone();
        tokio::spawn(async move {
            if let Err(err) = run_mainnet_explorer_tail(clickhouse).await {
                eprintln!("mainnet explorer tail error: {err:?}");
            }
        });
        Ok(())
    }

    // fn spawn_backfill(&self) {
    //     println!("backfill called");
    //     let clickhouse = self.clickhouse.clone();
    //     tokio::spawn(async move {
    //         if let Err(err) = backfill::run(clickhouse).await {
    //             eprintln!("delegation backfill error: {err:?}");
    //         }
    //     });
    // }

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
    tokio::task::spawn_blocking(move || -> Result<(String, Vec<SetBalancesData>)> {
        let oracle = OracleStakers::new(&ticker).build()?.send()?;
        let tx_id = oracle.clone().last_update()?;
        let data = parse_flp_balances_setting_res(&tx_id)?;
        Ok((tx_id, data))
    })
    .await?
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
    tokio::task::spawn_blocking(move || get_delegation_mappings(Some(limit), None)).await?
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
            ts,
            height,
            tx_id: tx_id.clone(),
            wallet_from: row.wallet_from,
            wallet_to: row.wallet_to,
            factor: row.factor,
        })
        .collect())
}

async fn run_mainnet_worker(
    clickhouse: Clickhouse,
    protocol: DataProtocol,
    start: u32,
) -> Result<()> {
    let protocol_name = protocol_label(protocol).to_string();
    let mut height = start;
    let mut cursor = None;
    if let Some(state) = clickhouse.fetch_mainnet_block_state(&protocol_name).await? {
        height = state.last_complete_height.max(start);
        if !state.last_cursor.is_empty() {
            cursor = Some(state.last_cursor);
        } else {
            height = height.saturating_add(1);
        }
    }
    println!("mainnet protocol {protocol_name} starting at height {height}");
    let mut network_tip = fetch_network_height().await.unwrap_or(height as u64);
    loop {
        while height as u64 + ARWEAVE_TIP_SAFE_GAP > network_tip {
            match fetch_network_height().await {
                Ok(latest) => network_tip = latest,
                Err(err) => {
                    eprintln!("mainnet tip fetch error protocol={protocol_name} err={err:?}");
                }
            }
            if height as u64 + ARWEAVE_TIP_SAFE_GAP > network_tip {
                println!(
                    "mainnet protocol {protocol_name} waiting, height {height} exceeds tip {network_tip} with gap {ARWEAVE_TIP_SAFE_GAP}"
                );
                sleep(Duration::from_secs(60)).await;
            }
        }
        let page = match fetch_mainnet_page(protocol, height, cursor.clone()).await {
            Ok(page) => page,
            Err(err) => {
                if is_empty_block_error(&err) {
                    cursor = None;
                    println!("mainnet protocol {protocol_name} height {height} empty");
                    let state_row = MainnetBlockStateRow {
                        updated_at: Utc::now(),
                        protocol: protocol_name.clone(),
                        last_complete_height: height,
                        last_cursor: String::new(),
                    };
                    clickhouse.insert_mainnet_block_state(&[state_row]).await?;
                    height = height.saturating_add(1);
                } else {
                    eprintln!(
                        "mainnet fetch error protocol={protocol_name} height={height} err={err:?}"
                    );
                    let delay = if is_rate_limit_error(&err) {
                        Duration::from_secs(5)
                    } else {
                        Duration::from_secs(1)
                    };
                    sleep(delay).await;
                }
                continue;
            }
        };
        let ts = Utc::now();
        let mut message_rows = Vec::with_capacity(page.mappings.len());
        let mut tag_rows = Vec::new();
        for meta in page.mappings {
            let MainnetBlockMessagesMeta {
                msg_id,
                owner,
                recipient,
                block_height,
                block_timestamp,
                bundled_in,
                data_size,
                tags,
            } = meta;
            let msg_id_for_tags = msg_id.clone();
            message_rows.push(MainnetMessageRow {
                ts,
                protocol: protocol_name.clone(),
                block_height,
                block_timestamp,
                msg_id,
                owner,
                recipient,
                bundled_in,
                data_size,
            });
            for tag in tags {
                tag_rows.push(MainnetMessageTagRow {
                    ts,
                    protocol: protocol_name.clone(),
                    block_height,
                    msg_id: msg_id_for_tags.clone(),
                    tag_key: tag.key,
                    tag_value: tag.value,
                });
            }
        }
        clickhouse.insert_mainnet_messages(&message_rows).await?;
        clickhouse.insert_mainnet_message_tags(&tag_rows).await?;
        cursor = if page.has_next_page {
            page.end_cursor.clone()
        } else {
            None
        };
        let state_row = MainnetBlockStateRow {
            updated_at: ts,
            protocol: protocol_name.clone(),
            last_complete_height: height,
            last_cursor: cursor.clone().unwrap_or_default(),
        };
        clickhouse.insert_mainnet_block_state(&[state_row]).await?;
        println!(
            "mainnet protocol {} height {} stored {} msgs",
            protocol_name,
            height,
            message_rows.len()
        );
        if cursor.is_none() {
            height = height.saturating_add(1);
        }
        sleep(Duration::from_secs(1)).await;
    }
}

async fn run_token_worker(clickhouse: Clickhouse, token: TokenConfig) -> Result<()> {
    let mut height = token.start_height;
    if let Some(state) = clickhouse.fetch_ao_token_block_state(token.label).await? {
        height = state
            .last_complete_height
            .max(token.start_height)
            .saturating_add(1);
    }
    println!("token indexer {} starting at height {height}", token.label);
    let mut network_tip = fetch_network_height().await.unwrap_or(height as u64);
    loop {
        while height as u64 + ARWEAVE_TIP_SAFE_GAP > network_tip {
            match fetch_network_height().await {
                Ok(latest) => network_tip = latest,
                Err(err) => {
                    eprintln!("token {} tip fetch error err={err:?}", token.label);
                }
            }
            if height as u64 + ARWEAVE_TIP_SAFE_GAP > network_tip {
                println!(
                    "token {} waiting, height {height} exceeds tip {network_tip} with gap {ARWEAVE_TIP_SAFE_GAP}",
                    token.label
                );
                sleep(Duration::from_secs(60)).await;
            }
        }

        let transfer_count = match ingest_token_query(
            &clickhouse,
            token,
            AoTokenQuery::Transfer,
            height,
            "transfer",
        )
        .await
        {
            Ok(count) => count,
            Err(err) => {
                if is_rate_limit_error(&err)
                    || is_timeout_error(&err)
                    || is_retryable_http_error(&err)
                    || is_not_found_error(&err)
                {
                    eprintln!(
                        "token {} transfer query error height={height} err={err:?}",
                        token.label
                    );
                    sleep(Duration::from_secs(300)).await;
                    continue;
                }
                return Err(err);
            }
        };
        let process_count =
            match ingest_token_query(&clickhouse, token, AoTokenQuery::Process, height, "process")
                .await
            {
                Ok(count) => count,
                Err(err) => {
                if is_rate_limit_error(&err)
                    || is_timeout_error(&err)
                    || is_retryable_http_error(&err)
                    || is_not_found_error(&err)
                {
                        eprintln!(
                            "token {} process query error height={height} err={err:?}",
                            token.label
                        );
                        sleep(Duration::from_secs(300)).await;
                        continue;
                    }
                    return Err(err);
                }
            };

        let state_row = AoTokenBlockStateRow {
            token: token.label.to_string(),
            last_complete_height: height,
            updated_at: Utc::now(),
        };
        clickhouse.insert_ao_token_block_state(&[state_row]).await?;
        println!(
            "token {} height {height} stored {transfer_count} transfers {process_count} process msgs",
            token.label
        );
        height = height.saturating_add(1);
        sleep(Duration::from_secs(1)).await;
    }
}

pub async fn fetch_mainnet_page(
    protocol: DataProtocol,
    height: u32,
    cursor: Option<String>,
) -> Result<MainnetBlockMessagesPage> {
    tokio::task::spawn_blocking(move || {
        scan_arweave_block_for_msgs(protocol, height, cursor.as_deref())
    })
    .await?
}

pub async fn fetch_ao_token_page(
    process_id: &'static str,
    query: AoTokenQuery,
    height: u32,
    cursor: Option<String>,
) -> Result<AoTokenMessagesPage> {
    tokio::task::spawn_blocking(move || {
        scan_arweave_block_for_token_msgs(process_id, query, height, cursor.as_deref())
    })
    .await?
}

pub async fn fetch_network_height() -> Result<u64> {
    tokio::task::spawn_blocking(|| get_network_height()).await?
}

pub fn protocol_label(protocol: DataProtocol) -> &'static str {
    match protocol {
        DataProtocol::A => "A",
        DataProtocol::B => "B",
    }
}

pub fn is_empty_block_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    msg.contains("no ao message id found")
}

fn is_rate_limit_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("http status: 429")
}

fn is_timeout_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    msg.contains("timeout") || msg.contains("timed out")
}

fn is_retryable_http_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string();
    let Some(status_part) = msg.split("http status: ").nth(1) else {
        return false;
    };
    let status_text = status_part.split_whitespace().next().unwrap_or("");
    let Ok(status) = status_text.parse::<u16>() else {
        return false;
    };
    (500..600).contains(&status)
}

fn is_not_found_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("http status: 404")
}

async fn ingest_token_query(
    clickhouse: &Clickhouse,
    token: TokenConfig,
    query: AoTokenQuery,
    height: u32,
    source: &str,
) -> Result<usize> {
    let mut cursor = None;
    let mut total = 0usize;
    loop {
        let page = fetch_ao_token_page(token.process_id, query, height, cursor.clone()).await?;
        let ts = Utc::now();
        let mut message_rows = Vec::with_capacity(page.mappings.len());
        let mut tag_rows = Vec::new();
        for meta in page.mappings {
            let AoTokenMessageMeta {
                msg_id,
                owner,
                recipient,
                block_height,
                block_timestamp,
                bundled_in,
                data_size,
                tags,
            } = meta;
            let msg_id_for_tags = msg_id.clone();
            message_rows.push(AoTokenMessageRow {
                ts,
                token: token.label.to_string(),
                source: source.to_string(),
                block_height,
                block_timestamp,
                msg_id,
                owner,
                recipient,
                bundled_in,
                data_size,
            });
            for tag in tags {
                tag_rows.push(AoTokenMessageTagRow {
                    ts,
                    token: token.label.to_string(),
                    source: source.to_string(),
                    block_height,
                    msg_id: msg_id_for_tags.clone(),
                    tag_key: tag.key,
                    tag_value: tag.value,
                });
            }
        }
        total += message_rows.len();
        clickhouse.insert_ao_token_messages(&message_rows).await?;
        clickhouse.insert_ao_token_message_tags(&tag_rows).await?;
        if page.has_next_page {
            if page.end_cursor.is_none() {
                break;
            }
            cursor = page.end_cursor.clone();
        } else {
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    Ok(total)
}

async fn run_mainnet_explorer_tail(clickhouse: Clickhouse) -> Result<()> {
    let last_row = clickhouse.latest_mainnet_explorer_row().await?;
    let mut last_height = last_row.as_ref().map(|r| r.height as u32).unwrap_or(0);
    let mut tx_roll = last_row.as_ref().map(|r| r.tx_count_rolling).unwrap_or(0);
    let mut proc_roll = last_row.as_ref().map(|r| r.processes_rolling).unwrap_or(0);
    let mut mod_roll = last_row.as_ref().map(|r| r.modules_rolling).unwrap_or(0);
    loop {
        let metrics = clickhouse
            .fetch_mainnet_block_metrics(last_height, 512)
            .await?;
        if metrics.is_empty() {
            sleep(Duration::from_secs(120)).await;
            continue;
        }
        let mut rows = Vec::with_capacity(metrics.len());
        for metric in metrics {
            last_height = metric.height;
            tx_roll += metric.tx_count;
            proc_roll += metric.new_process_count;
            mod_roll += metric.new_module_count;
            rows.push(MainnetExplorerRow {
                ts: metric.ts,
                height: metric.height as u64,
                tx_count: metric.tx_count,
                eval_count: metric.eval_count,
                transfer_count: metric.transfer_count,
                new_process_count: metric.new_process_count,
                new_module_count: metric.new_module_count,
                active_users: metric.active_users,
                active_processes: metric.active_processes,
                tx_count_rolling: tx_roll,
                processes_rolling: proc_roll,
                modules_rolling: mod_roll,
            });
        }
        clickhouse.insert_mainnet_explorer_rows(&rows).await?;
    }
}
