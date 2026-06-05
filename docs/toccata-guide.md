# Kaspa Toccata Hardfork Node Setup Guide

Kaspa is about to take a significant leap with the **Toccata Hardfork**, as detailed in [KIP16](https://github.com/kaspanet/kips/blob/master/kip-0016.md), [KIP17](https://github.com/kaspanet/kips/blob/master/kip-0017.md), [KIP20](https://github.com/kaspanet/kips/blob/master/kip-0020.md), and [KIP21](https://github.com/kaspanet/kips/blob/master/kip-0021.md), bringing native L1 covenant programming and infrastructure for based ZK applications to Kaspa. The hard fork is scheduled to activate on mainnet at DAA score `474,165,565`, which is roughly June 30, 2026 at 16:15 UTC.

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

### Note on The Change of Hardware Specifications

There are two reasons for the change:
1. [More accurate measurements](https://github.com/elldeeone/node-research/blob/main/investigations/node-resource-usage/REPORT.md#6-recommended-hardware-requirements) made by @elldeeone.
2. Doubling of the transient mass limit to allow zk-stark proofs.

## Running Your Node

1. **Obtain Kaspa v2.0.0 binaries**  
    Download and extract the official [2.0.0 release](https://github.com/kaspanet/rusty-kaspa/releases/tag/v2.0.0), or build from the `master` branch by following the instructions in the project README.

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
  - `--rpclisten=0.0.0.0` to listen for RPC connections on all network interfaces (public RPC). Use `--rpclisten=127.0.0.1` if you are running your pool/exchange software on the same machine
  - `--rpclisten-borsh` for local borsh RPC access from the `kaspa-cli` binary.
  - `--unsaferpc` for allowing P2P peer query and management via RPC (recommended to use only if **not** exposing RPC publicly).
  - `--perf-metrics --loglevel=info,kaspad_lib::daemon=debug,kaspa_mining::monitor=debug` for detailed performance logs.
  - `--loglevel=kaspa_grpc_server=warn` for suppressing most RPC connect/disconnect log reports.
  - `--ram-scale=3.0` for increasing cache size threefold (relevant for utilizing large RAM; can be set between 0.1 and 10).

## Mining and Preparation for Toccata

Toccata introduces v1 transactions with new fields (`TransactionOutput.covenant`, `TransactionInput.compute_commit`), which need to be preserved from the template that you get from `GetBlockTemplate` and passed back when you submit your mined block via `SubmitBlock`.

Ensure your pool/stratum is updated to preserve the new fields in the transactions by updating your GRPC proto files. Then, ensure that your pool software properly sends back the `mass` for each transaction in the block.

### Updating your Pool/Stratum to work in Toccata

#### Updating GRPC proto

Make sure that you get the updated `message.proto` and `rpc.proto` from the rusty-kaspa repo at https://github.com/kaspanet/rusty-kaspa/tree/master/rpc/grpc/core/proto

#### Using the golang Kaspad repo as an SDK

If you use the golang Kaspad repo as an SDK, a new tag [v0.12.23](https://github.com/kaspanet/kaspad/releases/tag/v0.12.23) has been made which contains the changes you would need to get your pool/stratum updated. Update your dependency on the old repo to that tag.

### What happens if I don't update my pool/stratum?

After Toccata activates, if you still have not updated, the blocks that you submit will be considered invalid.

## Test Against Testnet-10 Before Activation

Exchanges, miners, pools, explorers, wallet operators and other service providers should test their full infrastructure against Testnet-10 before Toccata activates on mainnet.

Ideally, this includes deposit and withdrawal flows, block template handling, mined block submission, transaction parsing, indexing, wallet balance tracking, fee estimation, and any internal services that depend on transaction or block formats. Testing on Testnet-10 is the recommended way to verify that your systems handle the Toccata transaction fields before mainnet activation.

## Fee adaptation
One week before Toccata activation the minimum fee rate is going to be increased from 1 sompi/gram to 100 sompi/gram. If you use the RPC fee estimation API to set your fees (most wallets do), you don't need to change anything, otherwise, you'll need to update your code to create transactions with the correct fee rate.

## Deprecation of `Transaction.mass` in Transaction APIs

The transaction field previously exposed as `mass` is now named `storage_mass` in Rust/protobuf APIs and `storageMass` in JSON/JavaScript APIs. This rename makes it clear that the field is the transaction's storage mass commitment, and avoids ambiguity with other mass concepts such as compute mass and transient mass.

For JSON RPC transaction objects, both `mass` and `storageMass` are currently emitted with the same value for backward compatibility. When deserializing JSON, clients may provide either field. If both are provided, they must agree: conflicting values are rejected. New integrations should read and write `storageMass`.

For JavaScript/WASM transaction objects, `mass` is deprecated and aliases `storageMass`. New code should use `storageMass`.
