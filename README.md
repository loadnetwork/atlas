## About

A monitoring and indexing system for the [Fairlaunch Bridge](https://ao.arweave.net/#/mint) and its associated active [Fairlaunch Projects (FLPs)](https://ao.arweave.net/#/delegate/) - checkout the monitored FLPs [here](./crates/common/src/projects.rs)


## Using the HTTP API

Base endpoint: https://api.load.network/atlas/ 

#### Example requests:

- `GET /` – health info.
- `GET /wallet/delegations/{ar_address}` – latest Set-Delegation payload for a wallet.
- `GET /wallet/delegation-mappings/{ar_address}` - delegation preference history over Arweave blockheight, goes back to the start of _delegation process deployment.
- `GET /wallet/identity/eoa/{eoa}` - returns the list of Arweave addresses associated with an EOA (bridge's identity linkage lookup)
- `GET /wallet/identity/ar-wallet/{ar_address}` - reverse proxy of `/eoa/{eoa}`
- `GET /oracle/{ticker}` – raw `Set-Balances` data payload for `usds`, `dai`, or `steth` oracles.
- `GET oracle/feed/{ticker}` - returns the recent indexed oracle feeds -aggregated- with additional metadata
- `GET /flp/delegators/{pid}` – merged snapshot of all tickers (LSTs + AR) delegating to a given FLP, including wallet/EVM mapping, factors, token amounts, and AR amounts.
- `GET /flp/delegators/multi?limit=100` - returns a list of delegators that delegate to at least 2 distinct FLPs.
- `GET /flp/minting/{project}` - returns the latest FLP's cycle `Own-Minting-Report` data
- `GET /flp/metadata/all` - return a vector of the tracked FLPs and their metadata

## Using the workspace crates in Rust


```toml
[dependencies]
atlas-common = { package = "common", git = "https://github.com/loadnetwork/atlas" }
atlas-flp = { package = "flp", git = "https://github.com/loadnetwork/atlas" }
```

Examples:

### `common`

```rust
use common::gql::OracleStakers;
let oracle = OracleStakers::new("usds").build()?.send()?;
let tx_id = oracle.last_update()?;
  ```
### `flp`

```rust
use flp::wallet::get_wallet_delegations;
let prefs = get_wallet_delegations("wallet_ar_address")?;
```

## License
Licensed at your option under either of:

* [Apache License, Version 2.0](./LICENSE-APACHE)
* [MIT License](./LICENSE-MIT)

## Contribution
Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
