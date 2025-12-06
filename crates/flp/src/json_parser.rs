use crate::types::OwnMintingReport;
use anyhow::Error;
use common::gateway::download_tx_data;

pub fn parse_own_minting_report(txid: &str) -> Result<OwnMintingReport, Error> {
    let tx_data = download_tx_data(txid)?;
    let mut res: OwnMintingReport = serde_json::from_slice(&tx_data)?;
    res.report_id = Some(txid.to_string());
    Ok(res)
}

#[cfg(test)]

mod tests {
    use crate::json_parser::parse_own_minting_report;

    #[test]
    fn parse_own_minting_report_test() {
        let repord_id: &str = "qEHaLBb8hXGi031STUId9MkVQWqfHdMt50qTbVhkiIo";
        let report = parse_own_minting_report(repord_id).unwrap();
        println!("{:?}", report);
        assert_eq!(report.timestamp, 1764976437232);
    }
}
