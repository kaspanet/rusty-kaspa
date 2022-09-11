#!/bin/sh -ex
rustc --version
cargo install cargo-fuzz

cargo fuzz run u128 --release -- -use_counters=1 -use_value_profile=1 "$@"
