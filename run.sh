#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONFIG_PATH="${ATLAS_CONFIG:-${ROOT_DIR}/atlas.toml}"

(cd "${ROOT_DIR}/crates/indexer" && ATLAS_CONFIG="${CONFIG_PATH}" cargo run --bin indexer) &
INDEXER_PID=$!

trap "kill ${INDEXER_PID}" EXIT

cd "${ROOT_DIR}/crates/server"
ATLAS_CONFIG="${CONFIG_PATH}" cargo run