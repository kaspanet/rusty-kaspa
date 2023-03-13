cargo fmt --all
cargo clippy --workspace --tests --benches -- -D warnings

$crates = @(
  "kaspa-core",
  "consensus-core",
  "addresses",
  "hashes",
  "math",
  "kaspa-rpc-core",
  "kaspa-bip32",
  "kaspa-wrpc-client",
  "kaspa-wrpc-wasm",
  "kaspa-wallet-core",
  "kaspa-wallet-cli",
  "kaspa-wallet-cli-wasm",
  "kaspa-wasm"
)

foreach ($crate in $crates)
{
  cargo clippy -p $crate --target wasm32-unknown-unknown
}