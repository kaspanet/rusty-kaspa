# Kaspa Toccata Hardfork Node Setup Guide

Kaspa is about to take a significant leap with the **Toccata Hardfork**, as detailed in [KIP16](https://github.com/kaspanet/kips/blob/master/kip-0016.md), [KIP17](https://github.com/kaspanet/kips/blob/master/kip-0017.md), [KIP20](https://github.com/kaspanet/kips/blob/master/kip-0020.md), and [KIP21](https://github.com/kaspanet/kips/blob/master/kip-0021.md), bringing native L1 covenant programming and infrastructure for based ZK applications to Kaspa. The hard fork is scheduled to activate on mainnet at DAA score `474,165,565`, roughly on June 30, 2026, at 16:15 UTC.

## Key notes

- Node operators should upgrade to this version before the upcoming hard fork.

- Miners may use this release to sanity-test mining flows, but this does not replace testing on an activated testnet. Before activation, mainnet block templates only contain current transaction versions, so some Toccata-specific paths are only meaningfully exercised after activation.

- RPC transaction submission now applies the upcoming higher minimum standard fee rule: `100 sompi * max(compute grams, 2 * transaction bytes)`.

- The `2 * transaction bytes` term reflects the post-Toccata normalized transient-mass component.

- This fee rule is a node policy / mempool rule, not a consensus rule. Consensus does not have a fee policy; zero-fee transactions are and remain consensus-valid.

- Until activation, the higher minimum standard fee is enforced only for transactions submitted directly through RPC. Transactions received through P2P relay continue to follow the current pre-activation policy, but after activation they will also be rejected unless they meet the higher fee rule.

- Wallets and other transaction-submitting software should verify that they do not rely on outdated fixed minimum-fee assumptions. Software should derive the required minimum fee from the node API where possible, or otherwise be updated to match the new minimum standard fee rule.

- gRPC/protobuf integrators can update and test their integrations ahead of activation to make sure existing APIs continue to behave as expected and to prepare for Toccata compatibility.

- The node database upgrade is one-way. After upgrading a node database to this release, it cannot be downgraded back to an earlier version. Operators who need to return to an earlier version can resync, but larger operators and pools should account for that operational cost.

## Recommended Hardware Specifications

- **Minimum**:  
  - 8 CPU cores  
  - 16 GB RAM  
  - 640 GB SSD  
  - 10 MB/s (or ~80 Mbit/s) network bandwidth

- **Preferred for Higher Performance**:  
  - 12–16 CPU cores  
  - 32 GB RAM  
  - 1 TB SSD  
  - Higher network bandwidth for robust peer support

While the minimum specs suffice to sync and maintain a node, increasing CPU cores, RAM, storage, and bandwidth allows your node to serve as a stronger focal point on the network. This leads to faster initial block download (IBD) for peers syncing from your node and provides more leeway for future storage growth and optimization.

If you are a pool operator, it is _strongly recommended_ that you pick specs that are closer to the preferred specifications above.

### Note on the Updated Hardware Specifications

There are two reasons for the change:

1. [More accurate measurements](https://github.com/elldeeone/node-research/blob/main/investigations/node-resource-usage/REPORT.md#6-recommended-hardware-requirements) made by @elldeeone.
2. The doubling of the transient mass limit to allow ZK-STARK proofs.

## Running Your Node

1. **Obtain Kaspa v2.0.1 binaries**  
   Download and extract the official [2.0.1 or newer release](https://github.com/kaspanet/rusty-kaspa/releases/latest), or build from the `master` branch by following the instructions in the project README.

2. **Launch the Node**  

   ```
   kaspad --utxoindex
   ```

   *(If running from source code:)*  

   ```
   cargo run --bin kaspad --release -- --utxoindex
   ```

   To run on testnet, simply add `--testnet` at the end. For example:

   ```
   kaspad --utxoindex --testnet
   ```

Leave this process running. Closing it will stop your node. If you have other flags that you use for your current node, you may continue to use those.

- **Advanced Command-Line Options**:
  - `--disable-upnp` if you don't want your node to be automatically publicly connectable (if your router supports UPnP). Recommended for pools and exchanges.
  - `--rpclisten=0.0.0.0` to listen for RPC connections on all network interfaces (public RPC). Use `--rpclisten=127.0.0.1` if you are running your pool/exchange software on the same machine.
  - `--rpclisten-borsh` for local Borsh RPC access from the `kaspa-cli` binary.
  - `--unsaferpc` to allow P2P peer query and management via RPC (recommended only when RPC is not exposed publicly).
  - `--perf-metrics --loglevel=info,kaspad_lib::daemon=debug,kaspa_mining::monitor=debug` for detailed performance logs.
  - `--loglevel=kaspa_grpc_server=warn` to suppress most RPC connect/disconnect log reports.
  - `--ram-scale=3.0` to increase cache size threefold (relevant for utilizing large RAM; can be set between 0.1 and 10).

## Mining and Preparation for Toccata

Toccata introduces v1 transactions with new fields (`TransactionOutput.covenant`, `TransactionInput.compute_commit`), which need to be preserved from the template that you get from `GetBlockTemplate` and passed back when you submit your mined block via `SubmitBlock`.

Pool and stratum operators must update their software to preserve the new fields in transactions by using an updated SDK, or by regenerating their gRPC bindings after reviewing [gRPC/protobuf Changes to Review](#grpcprotobuf-changes-to-review).

Individual miners usually do not need to update SDKs or gRPC/protobuf bindings directly. Mine through an upgraded pool, or use the provided [Stratum Bridge](../bridge/docs/README.md) with an upgraded node.

See the pool/stratum operator and miner checklists in [Required changes](#required-changes) for the concrete upgrade steps.

## Block Reward Info RPC for Pools and Miners

`GetBlockRewardInfo` is a new RPC endpoint for pool accounting, miner reporting, and reward tracking by block hash. It accepts a block hash and returns:

- `header`: the queried block header.
- `blockColor`: `UNKNOWN`, `BLUE`, or `RED`.
- `confirmationCount`: populated once the block has known merge context.
- `mergingChainBlockHash`: populated once the block has known merge context.
- `rewardAmount`: populated only when `blockColor` is `BLUE`.

Use this endpoint when you need to determine the reward attributable to a mined block after it is merged.

## Test Against Testnet-10 Before Activation

Exchanges, miners, pools, explorers, wallet operators, and other service providers should test their full infrastructure against Testnet-10 before Toccata activates on mainnet.

Ideally, this includes deposit and withdrawal flows, block template handling, mined block submission, transaction parsing, indexing, wallet balance tracking, fee estimation, and any internal services that depend on transaction or block formats. Testing on Testnet-10 is the recommended way to verify that your systems handle the Toccata transaction fields before mainnet activation.

## Fee Adaptation

On Toccata activation, the minimum fee rate will increase from 1 sompi/gram to 100 sompi/gram. If you use the RPC fee estimation API to set your fees (most wallets do), you don't need to change anything. Otherwise, you'll need to update your code to create transactions with the correct fee rate.

If you use `kaspawallet` from the Go Kaspad repo, download the new [v0.12.23 release](https://github.com/kaspanet/kaspad/releases/tag/v0.12.23), which is adapted to the new minimum fee rate.

## Deprecation of `Transaction.mass` in Transaction APIs

The transaction field previously exposed as `mass` is now named `storage_mass` in Rust/protobuf APIs and `storageMass` in JSON/JavaScript APIs. This rename makes it clear that the field is the transaction's storage mass commitment, and avoids ambiguity with other mass concepts such as compute mass and transient mass.

For JSON RPC transaction objects, both `mass` and `storageMass` are currently emitted with the same value for backward compatibility. When deserializing JSON, clients may provide either field. If both are provided, they must agree: conflicting values are rejected. New integrations should read and write `storageMass`.

For JavaScript/WASM transaction objects, `mass` is deprecated and aliases `storageMass`. New code should use `storageMass`.

## gRPC/protobuf Changes to Review

Updated proto files: [`messages.proto`](../rpc/grpc/core/proto/messages.proto), [`rpc.proto`](../rpc/grpc/core/proto/rpc.proto).

- `RpcTransaction.mass` is now `RpcTransaction.storage_mass`.
- `RpcTransactionInput.computeBudget` was added. For transaction version `1`, `computeBudget` replaces `sigOpCount`.
- `RpcTransactionOutput.covenant` was added.
- UTXO entries can carry covenant IDs: `RpcUtxoEntry.covenant_id`.

## Required changes

### Network users

- If you run a node, upgrade to the [2.0.1 or newer release](https://github.com/kaspanet/rusty-kaspa/releases/latest) before activation.
- If you do not operate a node, wallet, exchange, pool, explorer, or other Kaspa infrastructure, no action is required.

### Wallet software

- If you use the RPC fee estimation API, no fee calculation change is required.
- If your software uses fixed fee assumptions, update the minimum standard fee calculation from `1 sompi` per gram to `100 sompi` per gram. The fee floor is `100 sompi * max(compute grams, 2 * transaction bytes)`. Without this change, direct transaction submissions to upgraded nodes can fail.
- If you construct transactions manually, review [Deprecation of `Transaction.mass` in Transaction APIs](#deprecation-of-transactionmass-in-transaction-apis) before reading or writing transaction mass fields.

### Exchanges

- Upgrade all nodes before activation.
- Use an updated SDK: [Go Kaspad v0.12.23](https://github.com/kaspanet/kaspad/releases/tag/v0.12.23) or [rusty-kaspa v2.0.1 or newer](https://github.com/kaspanet/rusty-kaspa/releases/latest). If you generate your own RPC bindings, regenerate them after reviewing [gRPC/protobuf Changes to Review](#grpcprotobuf-changes-to-review).
- Test deposit, withdrawal, indexing, balance tracking, and fee-estimation flows against Testnet-10.
- If transaction submission does not use the RPC fee estimation API, update fee calculation to `100 sompi * max(compute grams, 2 * transaction bytes)`.
- Update transaction parsing to accept transaction version `1` and write `storageMass` / `storage_mass` instead of `mass`.

### Pools

- Upgrade all nodes before activation.
- Use an updated SDK: [Go Kaspad v0.12.23](https://github.com/kaspanet/kaspad/releases/tag/v0.12.23) or [rusty-kaspa v2.0.1 or newer](https://github.com/kaspanet/rusty-kaspa/releases/latest). If you generate your own RPC bindings, regenerate them after reviewing [gRPC/protobuf Changes to Review](#grpcprotobuf-changes-to-review).
- Update pool, job-generation, and block-submission code to preserve post-Toccata template fields while building the solved block. In particular, do not drop or overwrite `RpcBlockHeader.version`, transaction version `1`, `RpcTransaction.storage_mass`, `RpcTransactionInput.computeBudget`, or `RpcTransactionOutput.covenant`.
- If your software serializes block-template transactions into custom job messages, extend those messages and their block-reconstruction path to round-trip the new fields.
- Test `GetBlockTemplate` -> mining work distribution -> solved block reconstruction -> `SubmitBlock` on Testnet-10 with post-activation templates. After Toccata activates, blocks that strip the new fields can be invalid.

### Miners

- If you mine through a pool, verify that the pool has upgraded before activation.
- If you mine against your own node, upgrade the node and use the provided [Stratum Bridge](../bridge/docs/README.md), or another Toccata-compatible mining stack.
- If you operate custom mining software that calls `GetBlockTemplate` and `SubmitBlock` directly, follow the pool/stratum operator requirements above.
- Test your mining path against Testnet-10 before mainnet activation where possible.

### Explorers, indexers, and API integrators

- Update your SDKs, or regenerate RPC bindings after reviewing [gRPC/protobuf Changes to Review](#grpcprotobuf-changes-to-review), so transaction version `1`, input `computeBudget`, output `covenant`, UTXO `covenant_id`, and `storageMass` / `storage_mass` are handled correctly.
- If your storage schema records full transaction, output, or UTXO data, make sure it can store covenant bindings and covenant IDs.
- Review `GetBlockRewardInfo` and `GetSeqCommitLaneProof` if your integration needs reward-accounting, block-color, or KIP-21 lane-proof data.
- Continue accepting the deprecated JSON `mass` field for backward compatibility where needed, but emit and document `storageMass` for new integrations.
