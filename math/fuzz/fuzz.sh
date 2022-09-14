#!/bin/sh -ex
rustc --version
cargo install cargo-fuzz
fuzzer="$1"
shift;
cargo fuzz run "$fuzzer" --release -- -use_counters=1 -use_value_profile=1 "$@" ../../../rusty-kaspa-corpus/math/"$fuzzer"
