use crate::BlockStats;
pub const ATLAS_AGG_STATS_START_BLOCK: u64 = 1802760;  // Nov 26 2025 00:07:14 AM (GMT)


// agg_stats last message:
//
//   {
//     "created_date": "2025-11-26 00:00:00",
//     "tx_count": 125657,
//     "eval_count": 69,
//     "transfer_count": 2902,
//     "new_process_count": 3,
//     "new_module_count": 0,
//     "active_users": 87,
//     "active_processes": 883,
//     "tx_count_rolling": 2771411066,
//     "processes_rolling": 540463,
//     "modules_rolling": 10157
//   }
// msg: https://aolink.ar.io/#/message/rKp0XV6Hdy5BdMZTooSKnfX3RtwE-CKhXaTPr1CWfHI

pub const LATEST_AGG_STATS_SET: BlockStats = BlockStats {
    // height: 1806550, // onchain msg block
    // timestamp: 1764594637, // onchain timestamp Mon, 01 Dec 2025 13:10:37 GMT (UTC)
    height: 1802758, // the blockheight that match the last agg_stats msg
    timestamp: 1764114408, // the timestamp of the matching blockheight
    tx_count: 125657,
    eval_count: 69,
    transfer_count: 2902,
    new_process_count: 3,
    new_module_count: 0,
    active_users: 87,
    active_processes: 883,
    tx_count_rolling: 2771411066,
    processes_rolling: 540463,
    modules_rolling: 10157
};
