#!/bin/sh -ex
rustc --version
cargo install cargo-fuzz

cargo fuzz run u3072 --debug-assertions --release -- -use_counters=1 -use_value_profile=1 "$@" ../../../../rusty-kaspa-corpus/muhash/u3072/
