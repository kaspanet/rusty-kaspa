cargo fmt --all
cargo clippy --workspace --tests --benches -- -D warnings

$crates = @(
  # "kaspa-core",
  # "consensus-core",
  # "addresses",
  # "hashes",
  # "math",
  # "kaspa-rpc-core",
  # "kaspa-bip32",
  # "kaspa-wrpc-client",
  "kaspa-wrpc-wasm",
  # "kaspa-wallet-core",
  # "kaspa-wallet-cli",
  "kaspa-wallet-cli-wasm",
  "kaspa-wasm"
)

$env:AR="llvm-ar"
foreach ($crate in $crates)
{
  Write-Output "`ncargo clippy -p $crate --target wasm32-unknown-unknown"
  cargo clippy -p $crate --target wasm32-unknown-unknown
  $status=$LASTEXITCODE
  if($status -ne 0) {
    Write-Output "`n--> wasm32 check of $crate failed`n"
    break
  }
}
$env:AR=""