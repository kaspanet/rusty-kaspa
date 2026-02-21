# zk-covenant-rollup

## Running the host binary

The host binary must be run from within the `host/` directory (it is a workspace root):

```bash
cd host && cargo run --release --features cuda -- --non-activity-blocks=N
```

## Lint / format / test

All commands must be run from within the `host/` directory (`cd host` first):

```bash
cd host && cargo clippy --all-targets -- -D warnings
cd host && cargo fmt --all --check
cd host && cargo test
```

## Git commits

Do NOT include a `Co-Authored-By` line in commit messages.
