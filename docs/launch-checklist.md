# Public launch checklist

## Decisions
- [x] **Launch order:** run the DAGKnight **testnet 13** publicly first (DAGKnight on from genesis). Mainnet
  (`aero-mainnet`) keeps `dagknight_activation: never()` until DAGKnight has run cleanly on testnet 13; launching
  mainnet then means either launching without DAGKnight or setting its activation before the genesis is public.
- [x] **Genesis frozen.** The mainnet genesis (`consensus/core/src/config/genesis.rs`) is verified by
  `cargo test -p aero-consensus-core genesis`. Once anyone else runs it, any change to it (message, timestamp,
  bits) forks the chain. Edit it, regenerate with `gen_mainnet_genesis`, and re-run the test *before* publishing.

## Before publishing
- [ ] Push the repo to its own GitHub repository (not the upstream `kaspanet` remote) and replace the upstream
  URLs in `Cargo.toml`, `README.md`, `CONTRIBUTING.md`, `docs/` and `wasm/`.
- [ ] CI green (`cargo check --tests --workspace --benches`, tests, clippy, fmt).
- [ ] Tag a release; `deploy.yaml` builds `aerod`, `aero-wallet`, `aero-miner`, `stratum-bridge`, `rothschild`
  for Linux, Windows and macOS.
- [ ] Publish a Docker image built from `docker/Dockerfile.aerod`.

## Network infrastructure (see `running-a-seed-node.md`)
- [ ] 2–3 seed nodes on static public IPs with the P2P port open.
- [ ] DNS seeder, then fill `dns_seeders` in `consensus/core/src/config/params.rs`.
- [ ] Public RPC node(s); optionally a wRPC resolver (`rpc/wrpc/client/Resolvers.toml`).
- [ ] Block explorer. Set `AERO_EXPLORER_URL` at build time so the CLI links to it.
- [ ] Faucet (testnet only).

## Mining (see `mining.md`)
- [ ] Release `aero-miner`; document solo and `stratum-bridge` setups.
- [ ] Mine the first blocks yourself so new nodes have something to sync.

## Verification on real machines
- [ ] A fresh node on another machine syncs from the seeds (IBD, then stays synced).
- [ ] A node syncs after pruning has kicked in.
- [ ] Wallet sends and receives a transaction between two machines.
