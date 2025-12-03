#!/usr/bin/env bash
set -euo pipefail

(cd crates/indexer && cargo run) &
INDEXER_PID=$!

trap "kill $INDEXER_PID" EXIT

cd crates/server
cargo run
