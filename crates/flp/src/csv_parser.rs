use crate::types::SetBalancesData;
use anyhow::Error;
use common::gateway::download_tx_data;
use csv::{Reader, StringRecord};

pub fn parse_flp_balances_setting_res(txid: &str) -> Result<Vec<SetBalancesData>, Error> {
    let mut res: Vec<SetBalancesData> = Vec::new();
    let data = download_tx_data(txid)?;
    let str_data = String::from_utf8(data)?;
    let mut rdr = Reader::from_reader(str_data.as_bytes());
    // setting custom header given ao's flp Set-Balance dont have headers
    rdr.set_headers(StringRecord::from(vec!["eoa", "amount", "ar_address"]));

    for row in rdr.deserialize() {
        let record: SetBalancesData = row?;
        res.push(record);
    }
    Ok(res)
}

#[cfg(test)]

mod tests {
    use crate::csv_parser::parse_flp_balances_setting_res;
    use common::gql::OracleStakers;

    #[test]
    fn parse_flp_balances_setting_res_test() {
        let res =
            parse_flp_balances_setting_res("VkkgrjyRunadgj7p0j2_Wo8dC2F3H1WCmHgV9BL0i2Y").unwrap();
        println!("parse response: {:#?}", res);
        assert!(res.len() > 0);
    }

    #[test]
    fn steth_set_balances_full_test() {
        let oracle = OracleStakers::new("steth").build().unwrap().send().unwrap();
        let last_update = oracle.last_update().unwrap();
        let set_balances_parsed_data = parse_flp_balances_setting_res(&last_update).unwrap();
        println!("{:#?}", set_balances_parsed_data);
        assert!(set_balances_parsed_data.len() > 0);
    }
}
