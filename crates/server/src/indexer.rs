use anyhow::{Error, anyhow};
use chrono::{DateTime, NaiveDate, Utc};
use clickhouse::Row;
use common::{
    constants::{AO_TOKEN_START, DATA_PROTOCOL_A_START, DATA_PROTOCOL_B_START, PI_TOKEN_START},
    env::get_env_var,
    mainnet::get_network_height,
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct AtlasIndexerClient {
    client: clickhouse::Client,
}

enum BindValue {
    Str(String),
    U64(u64),
    U32(u32),
}

impl BindValue {
    fn apply(self, query: clickhouse::query::Query) -> clickhouse::query::Query {
        match self {
            BindValue::Str(val) => query.bind(val),
            BindValue::U64(val) => query.bind(val),
            BindValue::U32(val) => query.bind(val),
        }
    }
}

impl AtlasIndexerClient {
    pub async fn new() -> Result<Self, Error> {
        let url = get_env_var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
        let user = get_env_var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
        let password = get_env_var("CLICKHOUSE_PASSWORD").unwrap_or_default();
        let database =
            get_env_var("CLICKHOUSE_DATABASE").unwrap_or_else(|_| "atlas_oracles".into());
        let admin = clickhouse::Client::default()
            .with_url(&url)
            .with_user(&user)
            .with_password(&password);
        let client = admin.clone().with_database(&database);
        ensure_schema(&admin, &client, &database).await?;
        Ok(Self { client })
    }

    pub async fn latest_project_snapshot(&self, project: &str) -> Result<ProjectSnapshot, Error> {
        let query = "\
            with latest as (\
                select ticker, max(ts) as ts \
                from flp_positions \
                where project = ? \
                group by ticker\
            ) \
            select p.ts, p.ticker, p.wallet, p.eoa, toString(p.project) as project, p.factor, p.amount, p.ar_amount \
            from flp_positions p \
            inner join latest l on p.ticker = l.ticker and p.ts = l.ts \
            where p.project = ? \
            order by p.ticker, p.amount desc";
        let rows = self
            .client
            .query(query)
            .bind(project)
            .bind(project)
            .fetch_all::<FlpPositionRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no delegations found for project {project}"));
        }
        let ts = rows.iter().map(|row| row.ts).max().unwrap();
        let totals = aggregate_totals(&rows);
        let delegators = rows
            .into_iter()
            .map(|row| Delegator {
                wallet: row.wallet,
                eoa: row.eoa,
                ticker: row.ticker,
                factor: row.factor,
                amount: row.amount,
                ar_amount: row.ar_amount,
            })
            .collect();
        Ok(ProjectSnapshot {
            project: project.to_string(),
            ts,
            totals,
            delegators,
        })
    }

    pub async fn wallet_identity_history(&self, wallet: &str) -> Result<Vec<IdentityLink>, Error> {
        let rows = self
            .client
            .query(
                "select wallet, eoa, ts \
                 from wallet_balances \
                 where wallet = ? \
                 order by ts desc",
            )
            .bind(wallet)
            .fetch_all::<IdentityRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn eoa_identity_history(&self, eoa: &str) -> Result<Vec<IdentityLink>, Error> {
        let rows = self
            .client
            .query(
                "select wallet, eoa, ts \
                 from wallet_balances \
                 where eoa = ? \
                 order by ts desc",
            )
            .bind(eoa)
            .fetch_all::<IdentityRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn oracle_snapshot_feed(
        &self,
        ticker: &str,
        limit: u64,
    ) -> Result<Vec<OracleSnapshot>, Error> {
        let rows = self
            .client
            .query(
                "select o.ts, o.ticker, o.tx_id, toFloat64(sum(toDecimal128(if(length(p.amount) = 0, '0', p.amount), 18))) as total, uniqExact(p.wallet) as delegators \
                 from oracle_snapshots o \
                 left join flp_positions p \
                   on p.ticker = o.ticker and p.ts = o.ts \
                 where o.ticker = ? \
                 group by o.ts, o.ticker, o.tx_id \
                 having total > 0 \
                 order by o.ts desc \
                 limit ?",
           )
            .bind(ticker)
            .bind(limit)
            .fetch_all::<OracleSnapshot>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no oracle snapshots found for ticker {ticker}"));
        }
        Ok(rows)
    }

    pub async fn wallet_delegation_mappings(
        &self,
        wallet: &str,
    ) -> Result<Vec<DelegationMappingHistory>, Error> {
        let rows = self
            .client
            .query(
                "select ts, height, tx_id, wallet_from, wallet_to, factor \
                 from delegation_mappings \
                 where wallet_from = ? \
                 order by height desc",
            )
            .bind(wallet)
            .fetch_all::<DelegationMappingRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no delegation mappings found for wallet {wallet}"));
        }
        let mut map = BTreeMap::new();
        for row in rows {
            let key = (row.height, row.tx_id.clone());
            let entry = map.entry(key).or_insert_with(|| DelegationMappingHistory {
                ts: row.ts,
                height: row.height,
                tx_id: row.tx_id.clone(),
                wallet: row.wallet_from.clone(),
                preferences: Vec::new(),
            });
            entry.preferences.push(DelegationPreference {
                wallet_to: row.wallet_to,
                factor: row.factor,
            });
        }
        let mut out: Vec<_> = map.into_values().collect();
        out.sort_by(|a, b| b.height.cmp(&a.height));
        Ok(out)
    }

    pub async fn latest_delegation_heights(
        &self,
        limit: u64,
    ) -> Result<Vec<DelegationHeight>, Error> {
        let rows = self
            .client
            .query(
                "select height, tx_id \
                 from delegation_mappings \
                 group by height, tx_id \
                 order by height desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<DelegationHeightRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no delegation mappings indexed yet"));
        }
        Ok(rows
            .into_iter()
            .map(|row| DelegationHeight {
                height: row.height,
                tx_id: row.tx_id,
            })
            .collect())
    }

    pub async fn multi_project_delegators(&self, limit: u64) -> Result<Vec<MultiDelegator>, Error> {
        let rows = self
            .client
            .query(
                "select wallet, any(eoa) as eoa, countDistinct(project) as project_count, \
                 groupUniqArray(project) as projects \
                 from flp_positions \
                 group by wallet \
                 having project_count >= 2 \
                 order by project_count desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<MultiDelegatorRow>()
            .await?;
        if rows.is_empty() {
            return Err(anyhow!("no multi project delegators found"));
        }
        Ok(rows
            .into_iter()
            .map(|row| MultiDelegator {
                wallet: row.wallet,
                eoa: row.eoa,
                project_count: row.project_count,
                projects: row.projects,
            })
            .collect())
    }

    pub async fn project_cycle_totals(
        &self,
        project: &str,
        ticker: Option<&str>,
        limit: u64,
    ) -> Result<Vec<ProjectCycleTotal>, Error> {
        let ticker_clause = if ticker.is_some() {
            " and p.ticker = ?"
        } else {
            ""
        };
        let query_str = format!(
            "select o.tx_id, p.ts, \
             sumIf(toFloat64(p.amount), p.ticker = 'usds') as usds_total, \
             sumIf(toFloat64(p.amount), p.ticker = 'dai') as dai_total, \
             sumIf(toFloat64(p.amount), p.ticker = 'steth') as steth_total \
             from flp_positions p \
             inner join oracle_snapshots o on o.ticker = p.ticker and o.ts = p.ts \
             where p.project = ?{ticker_clause} \
             group by o.tx_id, p.ts \
             order by p.ts desc \
             limit ?",
        );
        let mut query = self.client.query(&query_str);
        query = query.bind(project);
        if let Some(t) = ticker {
            query = query.bind(t);
        }
        let rows = query.bind(limit).fetch_all::<ProjectCycleTotal>().await?;
        if rows.is_empty() {
            return Err(anyhow!("no cycle totals found for project {project}"));
        }
        Ok(rows)
    }

    pub async fn recent_mainnet_messages(
        &self,
        protocol: Option<&str>,
        limit: u64,
    ) -> Result<Vec<MainnetMessage>, Error> {
        let where_clause = if protocol.is_some() {
            " where m.protocol = ?"
        } else {
            ""
        };
        let sql = format!(
            "select \
                m.protocol, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, \
                m.bundled_in, m.data_size, m.ts, \
                arrayFilter(x -> x.1 != '', groupArray(tuple(ifNull(t.tag_key, ''), ifNull(t.tag_value, '')))) as tags \
             from ao_mainnet_messages m \
             left join ao_mainnet_message_tags t \
               on t.protocol = m.protocol and t.block_height = m.block_height and t.msg_id = m.msg_id \
             {where_clause} \
             group by m.protocol, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, m.bundled_in, m.data_size, m.ts \
             order by m.block_height desc, m.msg_id desc \
             limit ?"
        );
        let mut query = self.client.query(&sql);
        if let Some(p) = protocol {
            query = query.bind(p);
        }
        let rows = query.bind(limit).fetch_all::<MainnetMessageRow>().await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn block_mainnet_messages(
        &self,
        protocol: Option<&str>,
        height: u32,
        limit: u64,
    ) -> Result<Vec<MainnetMessage>, Error> {
        let mut clauses = vec!["m.block_height = ?".to_string()];
        if protocol.is_some() {
            clauses.push("m.protocol = ?".into());
        }
        let where_clause = format!(" where {}", clauses.join(" and "));
        let sql = format!(
            "select \
                m.protocol, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, \
                m.bundled_in, m.data_size, m.ts, \
                arrayFilter(x -> x.1 != '', groupArray(tuple(ifNull(t.tag_key, ''), ifNull(t.tag_value, '')))) as tags \
             from ao_mainnet_messages m \
             left join ao_mainnet_message_tags t \
               on t.protocol = m.protocol and t.block_height = m.block_height and t.msg_id = m.msg_id \
             {where_clause} \
             group by m.protocol, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, m.bundled_in, m.data_size, m.ts \
             order by m.msg_id \
             limit ?"
        );
        let mut query = self.client.query(&sql).bind(height);
        if let Some(p) = protocol {
            query = query.bind(p);
        }
        let rows = query.bind(limit).fetch_all::<MainnetMessageRow>().await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn mainnet_messages_by_tag(
        &self,
        protocol: Option<&str>,
        tag_keys: &[String],
        tag_value: &str,
        limit: u64,
    ) -> Result<Vec<MainnetMessage>, Error> {
        if tag_keys.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = std::iter::repeat("?")
            .take(tag_keys.len())
            .collect::<Vec<_>>()
            .join(", ");
        let protocol_clause = if protocol.is_some() {
            " and m.protocol = ?"
        } else {
            ""
        };
        let sql = format!(
            "select \
                m.protocol, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, \
                m.bundled_in, m.data_size, m.ts, \
                arrayFilter(x -> x.1 != '', groupArray(tuple(ifNull(t.tag_key, ''), ifNull(t.tag_value, '')))) as tags \
             from ao_mainnet_messages m \
             inner join ao_mainnet_message_tags filter \
               on filter.protocol = m.protocol and filter.block_height = m.block_height and filter.msg_id = m.msg_id \
             left join ao_mainnet_message_tags t \
               on t.protocol = m.protocol and t.block_height = m.block_height and t.msg_id = m.msg_id \
             where filter.tag_key in ({placeholders}) and filter.tag_value = ?{protocol_clause} \
             group by m.protocol, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, m.bundled_in, m.data_size, m.ts \
             order by m.block_height desc, m.msg_id desc \
             limit ?"
        );
        let mut query = self.client.query(&sql);
        for key in tag_keys {
            query = query.bind(key);
        }
        query = query.bind(tag_value);
        if let Some(p) = protocol {
            query = query.bind(p);
        }
        let rows = query.bind(limit).fetch_all::<MainnetMessageRow>().await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn mainnet_indexing_info(&self) -> Result<Vec<MainnetProtocolInfo>, Error> {
        let message_rows = self
            .client
            .query(
                "select protocol, max(block_height) as block_height, max(ts) as indexed_at \
                 from ao_mainnet_messages \
                 group by protocol",
            )
            .fetch_all::<MainnetProgressRow>()
            .await?;
        let state_rows = self
            .client
            .query(
                "select protocol, last_complete_height, last_cursor, updated_at \
                 from ao_mainnet_block_state \
                 order by protocol, updated_at desc",
            )
            .fetch_all::<MainnetStateRow>()
            .await?;
        let mut state_map = std::collections::HashMap::new();
        for row in state_rows {
            state_map.entry(row.protocol.clone()).or_insert(row);
        }
        Ok(message_rows
            .into_iter()
            .map(|row| {
                let mut info: MainnetProtocolInfo = row.into();
                if let Some(state) = state_map.get(&info.protocol) {
                    info.last_processed_height = Some(state.last_complete_height);
                    info.last_cursor = Some(state.last_cursor.clone());
                    info.last_processed_at = Some(state.updated_at);
                }
                info
            })
            .collect())
    }

    pub async fn mainnet_explorer_blocks(&self, limit: u64) -> Result<Vec<ExplorerBlock>, Error> {
        let rows = self
            .client
            .query(
                "select ts, height, tx_count, eval_count, transfer_count, \
                 new_process_count, new_module_count, active_users, active_processes, \
                 tx_count_rolling, processes_rolling, modules_rolling \
                 from ao_mainnet_explorer \
                 order by height desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<ExplorerBlockRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn mainnet_daily_explorer_stats(
        &self,
        day: NaiveDate,
    ) -> Result<ExplorerDayStats, Error> {
        let start = day.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let end = day
            .succ_opt()
            .unwrap_or(day)
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let rows = self
            .client
            .query(
                "select count() as blocks, sum(tx_count) as txs, \
                 sum(eval_count) as evals, sum(transfer_count) as transfers, \
                 sum(new_process_count) as new_processes, sum(new_module_count) as new_modules, \
                 sum(active_users) as active_users, sum(active_processes) as active_processes, \
                 max(tx_count_rolling) as txs_roll, \
                 max(processes_rolling) as processes_roll, \
                 max(modules_rolling) as modules_roll \
                 from ao_mainnet_explorer \
                 where toUnixTimestamp(ts) >= ? and toUnixTimestamp(ts) < ?",
            )
            .bind(start)
            .bind(end)
            .fetch_all::<ExplorerDayAggRow>()
            .await?;
        let stats = rows.into_iter().next().unwrap_or(ExplorerDayAggRow {
            blocks: 0,
            txs: 0,
            evals: 0,
            transfers: 0,
            new_processes: 0,
            new_modules: 0,
            active_users: 0,
            active_processes: 0,
            txs_roll: 0,
            processes_roll: 0,
            modules_roll: 0,
        });
        Ok(ExplorerDayStats {
            day,
            processed_blocks: stats.blocks,
            txs: stats.txs,
            evals: stats.evals,
            transfers: stats.transfers,
            new_processes_over_blocks: stats.new_processes,
            new_modules_over_blocks: stats.new_modules,
            active_users_over_blocks: stats.active_users,
            active_processes_over_blocks: stats.active_processes,
            txs_roll: stats.txs_roll,
            processes_roll: stats.processes_roll,
            modules_roll: stats.modules_roll,
        })
    }

    pub async fn mainnet_recent_explorer_days(
        &self,
        limit: u64,
    ) -> Result<Vec<ExplorerDayStats>, Error> {
        let rows = self
            .client
            .query(
                "select toInt64(toUnixTimestamp(toStartOfDay(ts))) as day_ts, \
                 count() as blocks, sum(tx_count) as txs, \
                 sum(eval_count) as evals, sum(transfer_count) as transfers, \
                 sum(new_process_count) as new_processes, sum(new_module_count) as new_modules, \
                 sum(active_users) as active_users, sum(active_processes) as active_processes, \
                 max(tx_count_rolling) as txs_roll, \
                 max(processes_rolling) as processes_roll, \
                 max(modules_rolling) as modules_roll \
                 from ao_mainnet_explorer \
                 group by day_ts \
                 order by day_ts desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<ExplorerRecentDayRow>()
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                DateTime::<Utc>::from_timestamp(row.day_ts, 0).map(|dt| ExplorerDayStats {
                    day: dt.date_naive(),
                    processed_blocks: row.blocks,
                    txs: row.txs,
                    evals: row.evals,
                    transfers: row.transfers,
                    new_processes_over_blocks: row.new_processes,
                    new_modules_over_blocks: row.new_modules,
                    active_users_over_blocks: row.active_users,
                    active_processes_over_blocks: row.active_processes,
                    txs_roll: row.txs_roll,
                    processes_roll: row.processes_roll,
                    modules_roll: row.modules_roll,
                })
            })
            .collect())
    }

    pub async fn ao_token_indexing_info(
        &self,
        token: &str,
    ) -> Result<AoTokenIndexingInfo, Error> {
        let stats = self
            .client
            .query(
                "select \
                    count() as total_messages, \
                    countIf(source = 'transfer') as transfer_messages, \
                    countIf(source = 'process') as process_messages, \
                    ifNull(max(block_height), 0) as max_block_height, \
                    ifNull(max(ts), toDateTime64(0, 3)) as latest_indexed_at \
                 from ao_token_messages \
                 where token = ?",
            )
            .bind(token)
            .fetch_all::<AoTokenStatsRow>()
            .await?
            .into_iter()
            .next()
            .unwrap_or_default();
        let state = self
            .client
            .query(
                "select token, \
                    argMax(last_complete_height, s.updated_at) as last_complete_height, \
                    max(s.updated_at) as updated_at \
                 from ao_token_block_state as s \
                 where token = ? \
                 group by token",
            )
            .bind(token)
            .fetch_all::<AoTokenStateRow>()
            .await?
            .into_iter()
            .next();
        let arweave_tip = tokio::task::spawn_blocking(get_network_height)
            .await
            .ok()
            .and_then(|res| res.ok());
        let (last_processed_height, last_processed_at) = match state {
            Some(row) => (Some(row.last_complete_height), Some(row.updated_at)),
            None => (None, None),
        };
        let block_lag = match (arweave_tip, last_processed_height) {
            (Some(tip), Some(processed)) if tip >= processed as u64 => Some(tip - processed as u64),
            _ => None,
        };
        let max_block_height = if stats.max_block_height == 0 {
            None
        } else {
            Some(stats.max_block_height)
        };
        let latest_indexed_at = if stats.latest_indexed_at.timestamp() == 0 {
            None
        } else {
            Some(stats.latest_indexed_at)
        };
        Ok(AoTokenIndexingInfo {
            start_height: token_start(token),
            arweave_tip,
            last_processed_height,
            last_processed_at,
            max_block_height,
            latest_indexed_at,
            total_messages: stats.total_messages,
            transfer_messages: stats.transfer_messages,
            process_messages: stats.process_messages,
            block_lag,
        })
    }

    pub async fn ao_token_frequency(
        &self,
        token: &str,
        limit: u64,
    ) -> Result<AoTokenFrequencyInfo, Error> {
        let action_sql =
            "select tag_value, count() as cnt \
             from ao_token_message_tags \
             where token = ? and tag_key = 'Action' \
             group by tag_value \
             order by cnt desc";
        let sender_sql =
            "select tag_value, count() as cnt \
             from ao_token_message_tags \
             where token = ? and tag_key = 'Sender' \
             group by tag_value \
             order by cnt desc \
             limit ?";
        let recipient_sql =
            "select tag_value, count() as cnt \
             from ao_token_message_tags \
             where token = ? and tag_key = 'Recipient' \
             group by tag_value \
             order by cnt desc \
             limit ?";

        let action_rows = self
            .client
            .query(action_sql)
            .bind(token)
            .fetch_all::<AoTokenTagCountRow>()
            .await?;
        let sender_rows = self
            .client
            .query(sender_sql)
            .bind(token)
            .bind(limit)
            .fetch_all::<AoTokenTagCountRow>()
            .await?;
        let recipient_rows = self
            .client
            .query(recipient_sql)
            .bind(token)
            .bind(limit)
            .fetch_all::<AoTokenTagCountRow>()
            .await?;

        Ok(AoTokenFrequencyInfo {
            actions: action_rows
                .into_iter()
                .map(|row| AoTokenActionCount {
                    action: row.tag_value,
                    count: row.cnt,
                })
                .collect(),
            top_senders: sender_rows
                .into_iter()
                .map(|row| AoTokenTagCount {
                    value: row.tag_value,
                    count: row.cnt,
                })
                .collect(),
            top_recipients: recipient_rows
                .into_iter()
                .map(|row| AoTokenTagCount {
                    value: row.tag_value,
                    count: row.cnt,
                })
                .collect(),
        })
    }

    pub async fn ao_token_richlist(
        &self,
        token: &str,
        limit: u64,
    ) -> Result<AoTokenRichlist, Error> {
        let top_spenders_sql = "\
            select sender.tag_value as address, \
                   sum(toUInt128OrZero(qty.tag_value)) as total_quantity \
            from ao_token_message_tags sender \
            inner join ao_token_message_tags qty \
              on qty.token = sender.token and qty.source = sender.source and qty.block_height = sender.block_height \
             and qty.msg_id = sender.msg_id \
            inner join ao_token_message_tags action \
              on action.token = sender.token and action.source = sender.source and action.block_height = sender.block_height \
             and action.msg_id = sender.msg_id \
            where sender.token = ? \
              and sender.tag_key = 'Sender' \
              and qty.tag_key = 'Quantity' \
              and action.tag_key = 'Action' \
              and action.tag_value = 'Credit-Notice' \
            group by sender.tag_value \
            order by total_quantity desc \
            limit ?";
        let top_receivers_sql = "\
            select recipient.tag_value as address, \
                   sum(toUInt128OrZero(qty.tag_value)) as total_quantity \
            from ao_token_message_tags recipient \
            inner join ao_token_message_tags qty \
              on qty.token = recipient.token and qty.source = recipient.source and qty.block_height = recipient.block_height \
             and qty.msg_id = recipient.msg_id \
            inner join ao_token_message_tags action \
              on action.token = recipient.token and action.source = recipient.source and action.block_height = recipient.block_height \
             and action.msg_id = recipient.msg_id \
            where recipient.token = ? \
              and recipient.tag_key = 'Recipient' \
              and qty.tag_key = 'Quantity' \
              and action.tag_key = 'Action' \
              and action.tag_value = 'Debit-Notice' \
            group by recipient.tag_value \
            order by total_quantity desc \
            limit ?";
        let spenders = self
            .client
            .query(top_spenders_sql)
            .bind(token)
            .bind(limit)
            .fetch_all::<AoTokenSumRow>()
            .await?;
        let receivers = self
            .client
            .query(top_receivers_sql)
            .bind(token)
            .bind(limit)
            .fetch_all::<AoTokenSumRow>()
            .await?;
        Ok(AoTokenRichlist {
            top_spenders: spenders
                .into_iter()
                .map(|row| AoTokenQuantityRank {
                    address: row.address,
                    total_quantity: format_quantity_human(row.total_quantity),
                })
                .collect(),
            top_receivers: receivers
                .into_iter()
                .map(|row| AoTokenQuantityRank {
                    address: row.address,
                    total_quantity: format_quantity_human(row.total_quantity),
                })
                .collect(),
        })
    }

    pub async fn ao_token_messages(
        &self,
        token: &str,
        source: Option<&str>,
        action: Option<&str>,
        min_qty: Option<&str>,
        max_qty: Option<&str>,
        from_ts: Option<u64>,
        to_ts: Option<u64>,
        block_min: Option<u32>,
        block_max: Option<u32>,
        recipient: Option<&str>,
        sender: Option<&str>,
        order: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<AoTokenMessage>, Error> {
        let mut joins = Vec::new();
        let mut where_clauses = Vec::new();
        let mut join_binds: Vec<BindValue> = Vec::new();
        let mut where_binds: Vec<BindValue> = Vec::new();

        where_clauses.push("m.token = ?");
        where_binds.push(BindValue::Str(token.to_string()));

        if let Some(val) = action {
            joins.push(
                "inner join ao_token_message_tags action_filter \
                 on action_filter.token = m.token and action_filter.source = m.source and action_filter.block_height = m.block_height \
                 and action_filter.msg_id = m.msg_id and action_filter.tag_key = 'Action' \
                 and lowerUTF8(action_filter.tag_value) = lowerUTF8(?)"
                    .to_string(),
            );
            join_binds.push(BindValue::Str(val.to_string()));
        }
        if let Some(val) = recipient {
            joins.push(
                "inner join ao_token_message_tags recipient_filter \
                 on recipient_filter.token = m.token and recipient_filter.source = m.source and recipient_filter.block_height = m.block_height \
                 and recipient_filter.msg_id = m.msg_id and recipient_filter.tag_key = 'Recipient' \
                 and recipient_filter.tag_value = ?"
                    .to_string(),
            );
            join_binds.push(BindValue::Str(val.to_string()));
        }
        if let Some(val) = sender {
            joins.push(
                "inner join ao_token_message_tags sender_filter \
                 on sender_filter.token = m.token and sender_filter.source = m.source and sender_filter.block_height = m.block_height \
                 and sender_filter.msg_id = m.msg_id and sender_filter.tag_key = 'Sender' \
                 and sender_filter.tag_value = ?"
                    .to_string(),
            );
            join_binds.push(BindValue::Str(val.to_string()));
        }
        if min_qty.is_some() || max_qty.is_some() {
            joins.push(
                "inner join ao_token_message_tags qty_filter \
                 on qty_filter.token = m.token and qty_filter.source = m.source and qty_filter.block_height = m.block_height \
                 and qty_filter.msg_id = m.msg_id and qty_filter.tag_key = 'Quantity'"
                    .to_string(),
            );
        }
        if let Some(val) = min_qty {
            where_clauses.push("toUInt128OrZero(qty_filter.tag_value) >= toUInt128OrZero(?)");
            where_binds.push(BindValue::Str(val.to_string()));
        }
        if let Some(val) = max_qty {
            where_clauses.push("toUInt128OrZero(qty_filter.tag_value) <= toUInt128OrZero(?)");
            where_binds.push(BindValue::Str(val.to_string()));
        }
        if let Some(val) = source {
            where_clauses.push("m.source = ?");
            where_binds.push(BindValue::Str(val.to_string()));
        }
        if let Some(val) = from_ts {
            where_clauses.push("m.block_timestamp >= ?");
            where_binds.push(BindValue::U64(val));
        }
        if let Some(val) = to_ts {
            where_clauses.push("m.block_timestamp <= ?");
            where_binds.push(BindValue::U64(val));
        }
        if let Some(val) = block_min {
            where_clauses.push("m.block_height >= ?");
            where_binds.push(BindValue::U32(val));
        }
        if let Some(val) = block_max {
            where_clauses.push("m.block_height <= ?");
            where_binds.push(BindValue::U32(val));
        }

        let join_clause = if joins.is_empty() {
            String::new()
        } else {
            format!("\n{}", joins.join("\n"))
        };
        let where_clause = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("\nwhere {}", where_clauses.join(" and "))
        };
        let order_dir = match order {
            Some("asc") => "asc",
            _ => "desc",
        };
        let sql = format!(
            "select \
                m.source, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, \
                m.bundled_in, m.data_size, m.ts, \
                arrayFilter(x -> x.1 != '', groupArray(tuple(ifNull(t.tag_key, ''), ifNull(t.tag_value, '')))) as tags \
             from ao_token_messages m \
             left join ao_token_message_tags t \
               on t.token = m.token and t.source = m.source and t.block_height = m.block_height and t.msg_id = m.msg_id \
             {join_clause} \
             {where_clause} \
             group by m.source, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, m.bundled_in, m.data_size, m.ts \
             order by m.block_height {order_dir}, m.msg_id {order_dir} \
             limit ? offset ?"
        );
        let mut query = self.client.query(&sql);
        for bind in join_binds.into_iter().chain(where_binds.into_iter()) {
            query = bind.apply(query);
        }
        let rows = query
            .bind(limit)
            .bind(offset)
            .fetch_all::<AoTokenMessageRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn ao_token_message_by_id(
        &self,
        token: &str,
        msg_id: &str,
    ) -> Result<Vec<AoTokenMessage>, Error> {
        let sql = "\
            select \
                m.source, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, \
                m.bundled_in, m.data_size, m.ts, \
                arrayFilter(x -> x.1 != '', groupArray(tuple(ifNull(t.tag_key, ''), ifNull(t.tag_value, '')))) as tags \
             from ao_token_messages m \
             left join ao_token_message_tags t \
               on t.token = m.token and t.source = m.source and t.block_height = m.block_height and t.msg_id = m.msg_id \
             where m.token = ? and m.msg_id = ? \
             group by m.source, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, m.bundled_in, m.data_size, m.ts \
             order by m.block_height desc, m.msg_id desc";
        let rows = self
            .client
            .query(sql)
            .bind(token)
            .bind(msg_id)
            .fetch_all::<AoTokenMessageRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn ao_token_messages_by_tag(
        &self,
        token: &str,
        source: Option<&str>,
        tag_key: &str,
        tag_value: &str,
        limit: u64,
    ) -> Result<Vec<AoTokenMessage>, Error> {
        let source_clause = if source.is_some() {
            " and m.source = ?"
        } else {
            ""
        };
        let sql = format!(
            "select \
                m.source, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, \
                m.bundled_in, m.data_size, m.ts, \
                arrayFilter(x -> x.1 != '', groupArray(tuple(ifNull(t.tag_key, ''), ifNull(t.tag_value, '')))) as tags \
             from ao_token_messages m \
             inner join ao_token_message_tags filter \
               on filter.token = m.token and filter.source = m.source and filter.block_height = m.block_height and filter.msg_id = m.msg_id \
             left join ao_token_message_tags t \
               on t.token = m.token and t.source = m.source and t.block_height = m.block_height and t.msg_id = m.msg_id \
             where m.token = ? and filter.tag_key = ? and filter.tag_value = ?{source_clause} \
             group by m.source, m.block_height, m.block_timestamp, m.msg_id, m.owner, m.recipient, m.bundled_in, m.data_size, m.ts \
             order by m.block_height desc, m.msg_id desc \
             limit ?"
        );
        let mut query = self
            .client
            .query(&sql)
            .bind(token)
            .bind(tag_key)
            .bind(tag_value);
        if let Some(src) = source {
            query = query.bind(src);
        }
        let rows = query.bind(limit).fetch_all::<AoTokenMessageRow>().await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn latest_explorer_blocks(&self, limit: u64) -> Result<Vec<ExplorerBlock>, Error> {
        let rows = self
            .client
            .query(
                "select ts, height, tx_count, eval_count, transfer_count, \
                 new_process_count, new_module_count, active_users, active_processes, \
                 tx_count_rolling, processes_rolling, modules_rolling \
                 from atlas_explorer \
                 order by height desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<ExplorerBlockRow>()
            .await?;
        Ok(rows.into_iter().map(|row| row.into()).collect())
    }

    pub async fn daily_explorer_stats(&self, day: NaiveDate) -> Result<ExplorerDayStats, Error> {
        let start = day.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
        let end = day
            .succ_opt()
            .unwrap_or(day)
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let rows = self
            .client
            .query(
                "select count() as blocks, sum(tx_count) as txs, \
                 sum(eval_count) as evals, sum(transfer_count) as transfers, \
                 sum(new_process_count) as new_processes, sum(new_module_count) as new_modules, \
                 sum(active_users) as active_users, sum(active_processes) as active_processes, \
                 max(tx_count_rolling) as txs_roll, \
                 max(processes_rolling) as processes_roll, \
                 max(modules_rolling) as modules_roll \
                 from atlas_explorer \
                 where toUnixTimestamp(ts) >= ? and toUnixTimestamp(ts) < ?",
            )
            .bind(start)
            .bind(end)
            .fetch_all::<ExplorerDayAggRow>()
            .await?;
        let stats = rows.into_iter().next().unwrap_or(ExplorerDayAggRow {
            blocks: 0,
            txs: 0,
            evals: 0,
            transfers: 0,
            new_processes: 0,
            new_modules: 0,
            active_users: 0,
            active_processes: 0,
            txs_roll: 0,
            processes_roll: 0,
            modules_roll: 0,
        });
        Ok(ExplorerDayStats {
            day,
            processed_blocks: stats.blocks,
            txs: stats.txs,
            evals: stats.evals,
            transfers: stats.transfers,
            new_processes_over_blocks: stats.new_processes,
            new_modules_over_blocks: stats.new_modules,
            active_users_over_blocks: stats.active_users,
            active_processes_over_blocks: stats.active_processes,
            txs_roll: stats.txs_roll,
            processes_roll: stats.processes_roll,
            modules_roll: stats.modules_roll,
        })
    }

    pub async fn recent_explorer_days(&self, limit: u64) -> Result<Vec<ExplorerDayStats>, Error> {
        let rows = self
            .client
            .query(
                "select toInt64(toUnixTimestamp(toStartOfDay(ts))) as day_ts, \
                 count() as blocks, sum(tx_count) as txs, \
                 sum(eval_count) as evals, sum(transfer_count) as transfers, \
                 sum(new_process_count) as new_processes, sum(new_module_count) as new_modules, \
                 sum(active_users) as active_users, sum(active_processes) as active_processes, \
                 max(tx_count_rolling) as txs_roll, \
                 max(processes_rolling) as processes_roll, \
                 max(modules_rolling) as modules_roll \
                 from atlas_explorer \
                 group by day_ts \
                 order by day_ts desc \
                 limit ?",
            )
            .bind(limit)
            .fetch_all::<ExplorerRecentDayRow>()
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                DateTime::<Utc>::from_timestamp(row.day_ts, 0).map(|dt| ExplorerDayStats {
                    day: dt.date_naive(),
                    processed_blocks: row.blocks,
                    txs: row.txs,
                    evals: row.evals,
                    transfers: row.transfers,
                    new_processes_over_blocks: row.new_processes,
                    new_modules_over_blocks: row.new_modules,
                    active_users_over_blocks: row.active_users,
                    active_processes_over_blocks: row.active_processes,
                    txs_roll: row.txs_roll,
                    processes_roll: row.processes_roll,
                    modules_roll: row.modules_roll,
                })
            })
            .collect())
    }
}

async fn ensure_schema(
    admin: &clickhouse::Client,
    client: &clickhouse::Client,
    database: &str,
) -> Result<(), Error> {
    let create_db = format!("create database if not exists {database}");
    admin.query(&create_db).execute().await?;
    let stmts = [
        "create table if not exists oracle_snapshots(ts DateTime64(3), ticker String, tx_id String) engine=MergeTree order by (ticker, ts)",
        "create table if not exists wallet_balances(ts DateTime64(3), ticker String, wallet String, eoa String, amount String, tx_id String) engine=ReplacingMergeTree order by (ticker, wallet, ts)",
        "create table if not exists wallet_delegations(ts DateTime64(3), wallet String, payload String) engine=ReplacingMergeTree order by (wallet, ts)",
        "create table if not exists flp_positions(ts DateTime64(3), ticker String, wallet String, eoa String, project String, factor UInt32, amount String) engine=ReplacingMergeTree order by (project, wallet, ts)",
        "create table if not exists delegation_mappings(ts DateTime64(3), height UInt32, tx_id String, wallet_from String, wallet_to String, factor UInt32) engine=ReplacingMergeTree order by (height, tx_id, wallet_from, wallet_to)",
        "create table if not exists atlas_explorer(ts DateTime64(3), height UInt64, tx_count UInt64, eval_count UInt64, transfer_count UInt64, new_process_count UInt64, new_module_count UInt64, active_users UInt64, active_processes UInt64, tx_count_rolling UInt64, processes_rolling UInt64, modules_rolling UInt64) engine=ReplacingMergeTree order by height",
        "create table if not exists ao_token_messages(ts DateTime64(3), token String, source String, block_height UInt32, block_timestamp UInt64, msg_id String, owner String, recipient String, bundled_in String, data_size String) engine=ReplacingMergeTree order by (token, source, block_height, msg_id)",
        "create table if not exists ao_token_message_tags(ts DateTime64(3), token String, source String, block_height UInt32, msg_id String, tag_key String, tag_value String) engine=ReplacingMergeTree order by (token, source, tag_key, tag_value, block_height, msg_id)",
        "create table if not exists ao_token_block_state(token String, last_complete_height UInt32, updated_at DateTime64(3)) engine=ReplacingMergeTree order by (token, updated_at)",
    ];
    for stmt in stmts {
        client.query(stmt).execute().await?;
    }
    let alters = [
        "alter table wallet_balances add column if not exists eoa String after wallet",
        "alter table wallet_balances add column if not exists ar_balance String after amount",
        "alter table flp_positions add column if not exists eoa String after wallet",
        "alter table flp_positions add column if not exists ar_amount String after amount",
        "alter table flp_positions modify column project String",
        "alter table delegation_mappings add column if not exists ts DateTime64(3) default now()",
        "alter table ao_token_messages add column if not exists token String default 'ao'",
        "alter table ao_token_message_tags add column if not exists token String default 'ao'",
        "alter table ao_token_block_state add column if not exists token String default 'ao'",
    ];
    for stmt in alters {
        client.query(stmt).execute().await?;
    }
    Ok(())
}

fn aggregate_totals(rows: &[FlpPositionRow]) -> Vec<ProjectTotal> {
    let mut map = BTreeMap::new();
    for row in rows {
        let entry = map.entry(row.ticker.clone()).or_insert(ProjectTotal {
            ticker: row.ticker.clone(),
            amount: 0.0,
            ar_amount: 0.0,
            delegators_count: 0,
        });
        entry.amount += row.amount.parse::<f64>().unwrap_or(0.0);
        entry.ar_amount += row.ar_amount.parse::<f64>().unwrap_or(0.0);
        entry.delegators_count += 1;
    }
    map.into_values().collect()
}

#[derive(Row, serde::Deserialize)]
struct FlpPositionRow {
    #[allow(dead_code)]
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    ticker: String,
    wallet: String,
    eoa: String,
    #[allow(dead_code)]
    project: String,
    factor: u32,
    amount: String,
    ar_amount: String,
}

#[derive(Row, serde::Deserialize)]
struct IdentityRow {
    wallet: String,
    eoa: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
}

impl From<IdentityRow> for IdentityLink {
    fn from(value: IdentityRow) -> Self {
        IdentityLink {
            wallet: value.wallet,
            eoa: value.eoa,
            ts: value.ts,
        }
    }
}

#[derive(Serialize)]
pub struct ProjectSnapshot {
    pub project: String,
    pub ts: DateTime<Utc>,
    pub totals: Vec<ProjectTotal>,
    pub delegators: Vec<Delegator>,
}

#[derive(Serialize, Clone)]
pub struct ProjectTotal {
    pub ticker: String,
    pub amount: f64,
    pub delegators_count: u32,
    pub ar_amount: f64,
}

#[derive(Serialize, Clone)]
pub struct Delegator {
    pub wallet: String,
    pub eoa: String,
    pub ticker: String,
    pub factor: u32,
    pub amount: String,
    pub ar_amount: String,
}

#[derive(Serialize, Clone)]
pub struct IdentityLink {
    pub wallet: String,
    pub eoa: String,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub ts: DateTime<Utc>,
}

#[derive(Row, serde::Deserialize, Serialize, Clone)]
pub struct OracleSnapshot {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub ticker: String,
    pub tx_id: String,
    pub total: f64,
    pub delegators: u64,
}

#[derive(Row, serde::Deserialize)]
struct DelegationMappingRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    height: u32,
    tx_id: String,
    wallet_from: String,
    wallet_to: String,
    factor: u32,
}

#[derive(Serialize, Clone)]
pub struct DelegationMappingHistory {
    pub ts: DateTime<Utc>,
    pub height: u32,
    pub tx_id: String,
    pub wallet: String,
    pub preferences: Vec<DelegationPreference>,
}

#[derive(Serialize, Clone)]
pub struct DelegationPreference {
    pub wallet_to: String,
    pub factor: u32,
}

#[derive(Row, serde::Deserialize)]
struct DelegationHeightRow {
    height: u32,
    tx_id: String,
}

#[derive(Serialize, Clone)]
pub struct DelegationHeight {
    pub height: u32,
    pub tx_id: String,
}

#[derive(Row, serde::Deserialize)]
struct MultiDelegatorRow {
    wallet: String,
    eoa: String,
    project_count: u64,
    projects: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct MultiDelegator {
    pub wallet: String,
    pub eoa: String,
    pub project_count: u64,
    pub projects: Vec<String>,
}

#[derive(Row, serde::Deserialize, Serialize, Clone)]
pub struct ProjectCycleTotal {
    pub tx_id: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub usds_total: f64,
    pub dai_total: f64,
    pub steth_total: f64,
}

#[derive(Row, serde::Deserialize)]
struct ExplorerBlockRow {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    height: u64,
    tx_count: u64,
    eval_count: u64,
    transfer_count: u64,
    new_process_count: u64,
    new_module_count: u64,
    active_users: u64,
    active_processes: u64,
    tx_count_rolling: u64,
    processes_rolling: u64,
    modules_rolling: u64,
}

impl From<ExplorerBlockRow> for ExplorerBlock {
    fn from(row: ExplorerBlockRow) -> Self {
        ExplorerBlock {
            ts: row.ts,
            height: row.height,
            tx_count: row.tx_count,
            eval_count: row.eval_count,
            transfer_count: row.transfer_count,
            new_process_count: row.new_process_count,
            new_module_count: row.new_module_count,
            active_users: row.active_users,
            active_processes: row.active_processes,
            tx_count_rolling: row.tx_count_rolling,
            processes_rolling: row.processes_rolling,
            modules_rolling: row.modules_rolling,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ExplorerBlock {
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    pub ts: DateTime<Utc>,
    pub height: u64,
    pub tx_count: u64,
    pub eval_count: u64,
    pub transfer_count: u64,
    pub new_process_count: u64,
    pub new_module_count: u64,
    pub active_users: u64,
    pub active_processes: u64,
    pub tx_count_rolling: u64,
    pub processes_rolling: u64,
    pub modules_rolling: u64,
}

#[derive(Row, serde::Deserialize)]
struct ExplorerDayAggRow {
    blocks: u64,
    txs: u64,
    evals: u64,
    transfers: u64,
    new_processes: u64,
    new_modules: u64,
    active_users: u64,
    active_processes: u64,
    txs_roll: u64,
    processes_roll: u64,
    modules_roll: u64,
}

#[derive(Serialize, Clone)]
pub struct ExplorerDayStats {
    pub day: NaiveDate,
    pub processed_blocks: u64,
    pub txs: u64,
    pub evals: u64,
    pub transfers: u64,
    pub new_processes_over_blocks: u64,
    pub new_modules_over_blocks: u64,
    pub active_users_over_blocks: u64,
    pub active_processes_over_blocks: u64,
    pub txs_roll: u64,
    pub processes_roll: u64,
    pub modules_roll: u64,
}

#[derive(Row, serde::Deserialize)]
struct MainnetMessageRow {
    protocol: String,
    block_height: u32,
    block_timestamp: u64,
    msg_id: String,
    owner: String,
    recipient: String,
    bundled_in: String,
    data_size: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    tags: Vec<(String, String)>,
}

impl From<MainnetMessageRow> for MainnetMessage {
    fn from(row: MainnetMessageRow) -> Self {
        MainnetMessage {
            protocol: row.protocol,
            block_height: row.block_height,
            block_timestamp: row.block_timestamp,
            msg_id: row.msg_id,
            owner: row.owner,
            recipient: row.recipient,
            bundled_in: row.bundled_in,
            data_size: row.data_size,
            tags: row
                .tags
                .into_iter()
                .filter(|(k, _)| !k.is_empty())
                .map(|(key, value)| MainnetMessageTag { key, value })
                .collect(),
            indexed_at: row.ts,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct MainnetMessage {
    pub protocol: String,
    pub block_height: u32,
    pub block_timestamp: u64,
    pub msg_id: String,
    pub owner: String,
    pub recipient: String,
    pub bundled_in: String,
    pub data_size: String,
    pub tags: Vec<MainnetMessageTag>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub indexed_at: DateTime<Utc>,
}

#[derive(Serialize, Clone)]
pub struct MainnetMessageTag {
    pub key: String,
    pub value: String,
}

#[derive(Row, serde::Deserialize)]
struct AoTokenMessageRow {
    source: String,
    block_height: u32,
    block_timestamp: u64,
    msg_id: String,
    owner: String,
    recipient: String,
    bundled_in: String,
    data_size: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    ts: DateTime<Utc>,
    tags: Vec<(String, String)>,
}

impl From<AoTokenMessageRow> for AoTokenMessage {
    fn from(row: AoTokenMessageRow) -> Self {
        AoTokenMessage {
            source: row.source,
            block_height: row.block_height,
            block_timestamp: row.block_timestamp,
            msg_id: row.msg_id,
            owner: row.owner,
            recipient: row.recipient,
            bundled_in: row.bundled_in,
            data_size: row.data_size,
            tags: row
                .tags
                .into_iter()
                .filter(|(k, _)| !k.is_empty())
                .map(|(key, value)| AoTokenMessageTag { key, value })
                .collect(),
            indexed_at: row.ts,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct AoTokenMessage {
    pub source: String,
    pub block_height: u32,
    pub block_timestamp: u64,
    pub msg_id: String,
    pub owner: String,
    pub recipient: String,
    pub bundled_in: String,
    pub data_size: String,
    pub tags: Vec<AoTokenMessageTag>,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub indexed_at: DateTime<Utc>,
}

#[derive(Serialize, Clone)]
pub struct AoTokenMessageTag {
    pub key: String,
    pub value: String,
}

#[derive(Row, serde::Deserialize)]
struct MainnetProgressRow {
    protocol: String,
    block_height: u32,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    indexed_at: DateTime<Utc>,
}

#[derive(Row, serde::Deserialize, Default)]
struct AoTokenStatsRow {
    total_messages: u64,
    transfer_messages: u64,
    process_messages: u64,
    max_block_height: u32,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    latest_indexed_at: DateTime<Utc>,
}

#[derive(Row, serde::Deserialize)]
struct AoTokenStateRow {
    token: String,
    last_complete_height: u32,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    updated_at: DateTime<Utc>,
}

#[derive(Serialize, Clone)]
pub struct AoTokenIndexingInfo {
    pub start_height: u32,
    pub arweave_tip: Option<u64>,
    pub last_processed_height: Option<u32>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub last_processed_at: Option<DateTime<Utc>>,
    pub max_block_height: Option<u32>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub latest_indexed_at: Option<DateTime<Utc>>,
    pub total_messages: u64,
    pub transfer_messages: u64,
    pub process_messages: u64,
    pub block_lag: Option<u64>,
}

#[derive(Row, serde::Deserialize)]
struct AoTokenTagCountRow {
    tag_value: String,
    cnt: u64,
}

#[derive(Serialize, Clone)]
pub struct AoTokenActionCount {
    pub action: String,
    pub count: u64,
}

#[derive(Serialize, Clone)]
pub struct AoTokenTagCount {
    pub value: String,
    pub count: u64,
}

#[derive(Serialize, Clone)]
pub struct AoTokenFrequencyInfo {
    pub actions: Vec<AoTokenActionCount>,
    pub top_senders: Vec<AoTokenTagCount>,
    pub top_recipients: Vec<AoTokenTagCount>,
}

#[derive(Row, serde::Deserialize)]
struct AoTokenSumRow {
    address: String,
    total_quantity: u128,
}

#[derive(Serialize, Clone)]
pub struct AoTokenQuantityRank {
    pub address: String,
    pub total_quantity: String,
}

#[derive(Serialize, Clone)]
pub struct AoTokenRichlist {
    pub top_spenders: Vec<AoTokenQuantityRank>,
    pub top_receivers: Vec<AoTokenQuantityRank>,
}

fn format_quantity_human(value: u128) -> String {
    let scale: u128 = 1_000_000_000_000;
    let whole = value / scale;
    let frac = value % scale;
    if frac == 0 {
        return whole.to_string();
    }
    let mut frac_str = format!("{frac:012}");
    while frac_str.ends_with('0') {
        frac_str.pop();
    }
    format!("{whole}.{frac_str}")
}

impl From<MainnetProgressRow> for MainnetProtocolInfo {
    fn from(row: MainnetProgressRow) -> Self {
        let protocol = row.protocol;
        MainnetProtocolInfo {
            last_processed_height: None,
            last_processed_at: None,
            last_cursor: None,
            start_height: protocol_start(&protocol),
            protocol,
            block_height: row.block_height,
            indexed_at: row.indexed_at,
        }
    }
}

#[derive(Serialize, Clone)]
pub struct MainnetProtocolInfo {
    pub protocol: String,
    pub block_height: u32,
    pub start_height: u32,
    #[serde(with = "chrono::serde::ts_milliseconds")]
    pub indexed_at: DateTime<Utc>,
    pub last_processed_height: Option<u32>,
    #[serde(with = "chrono::serde::ts_milliseconds_option")]
    pub last_processed_at: Option<DateTime<Utc>>,
    pub last_cursor: Option<String>,
}

fn protocol_start(protocol: &str) -> u32 {
    match protocol {
        "A" => DATA_PROTOCOL_A_START,
        "B" => DATA_PROTOCOL_B_START,
        _ => 0,
    }
}

fn token_start(token: &str) -> u32 {
    match token {
        "ao" => AO_TOKEN_START,
        "pi" => PI_TOKEN_START,
        _ => 0,
    }
}

#[derive(Row, serde::Deserialize)]
struct ExplorerRecentDayRow {
    day_ts: i64,
    blocks: u64,
    txs: u64,
    evals: u64,
    transfers: u64,
    new_processes: u64,
    new_modules: u64,
    active_users: u64,
    active_processes: u64,
    txs_roll: u64,
    processes_roll: u64,
    modules_roll: u64,
}
#[derive(Row, serde::Deserialize)]
struct MainnetStateRow {
    protocol: String,
    last_complete_height: u32,
    last_cursor: String,
    #[serde(with = "clickhouse::serde::chrono::datetime64::millis")]
    updated_at: DateTime<Utc>,
}
