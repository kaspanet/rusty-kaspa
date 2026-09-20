#!/bin/bash
set -ex
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR/.."
cargo run --bin simpa --release -- -n 5000 -t 0 --blocks-json-gz-output-path testing/integration/testdata/dags_for_json_tests/goref-notx-5000-blocks/blocks.json.gz
cargo run --bin simpa --release -- -n 265 -t 4 --blocks-json-gz-output-path testing/integration/testdata/dags_for_json_tests/goref-1060-tx-265-blocks/blocks.json.gz
cargo run --bin simpa --release -- -n 5000 -t 1 --blocks-json-gz-output-path testing/integration/testdata/dags_for_json_tests/goref_custom_pruning_depth/blocks.json.gz --test-pruning --retention-period-days 100