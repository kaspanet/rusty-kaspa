# Kaspa Crescendo Hardfork Node Setup Guide

Kaspa is about to take a significant leap with the **Crescendo Hardfork**, as detailed in [KIP14](https://github.com/kaspanet/kips/blob/master/kip-0014.md), transitioning from 1 to 10 blocks per second. The hardfork is scheduled to activate on mainnet at DAA Score `110,165,000` which is roughly `2025-05-05 1500 UTC`.

## Recommended Hardware Specifications

- **Minimum**:  
  - 8 CPU cores  
  - 16 GB RAM  
  - 256 GB SSD  
  - 5 MB/s (or ~40 Mbit/s) network bandwidth

- **Preferred for Higher Performance**:  
  - 12–16 CPU cores  
  - 32 GB RAM  
  - 512 GB SSD  
  - Higher network bandwidth for robust peer support

While the minimum specs suffice to sync and maintain a 10 bps node, increasing CPU cores, RAM, storage, and bandwidth allows your node to serve as a stronger focal point on the network. This leads to faster initial block download (IBD) for peers syncing from your node and provides more leeway for future storage growth and optimization.

If you are a pool operator, it is _strongly recommended_ that you pick specs that are closer to the preferred specifications above.

## Running Your Node

1. **Obtain Kaspa v1.0.0 binaries**  
    Download and extract the official [1.0.0 release](https://github.com/kaspanet/rusty-kaspa/releases/tag/v1.0.0), or build from the `master` branch by following the instructions in the project README.

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

## Mining and Preparation for Crescendo

Crescendo introduces a new field in transactions, `mass`, which needs to be preserved from the template that you get from `GetBlockTemplate` and passed back when you submit your mined block via `SubmitBlock`.

Ensure your pool/stratum is updated to preserve this field in the transactions by updating your GRPC proto files. Then, ensure that your pool software properly sends back the `mass` for each transaction in the block.

### Updating your Pool/Stratum to work in Crescendo

#### Updating GRPC proto

Make sure that you get the updated `message.proto` and `rpc.proto` from the rusty-kaspa repo at https://github.com/kaspanet/rusty-kaspa/tree/master/rpc/grpc/core/proto

#### Using the golang Kaspad repo as an SDK

If you use the golang Kaspad repo as an SDK, a new tag [v0.12.20](https://github.com/kaspanet/kaspad/releases/tag/v0.12.20) has been made which contains the changes you would need to get your pool/stratum updated. Update your dependency on the old repo to that tag.

### What happens if I don't update my pool/stratum?

Before Crescendo activates, you will see the following warning in your node when you submit blocks:

```
The RPC submitted block {block_has} contains a transaction {tx_hash} with mass = 0 while it should have been strictly positive.
This indicates that the RPC conversion flow used by the miner does not preserve the mass values received from GetBlockTemplate.
You must upgrade your miner flow to propagate the mass field correctly prior to the Crescendo hardfork activation. 
Failure to do so will result in your blocks being considered invalid when Crescendo activates.
```

Double check that your proto files are updated and that you are able to submit blocks with transactions without triggering this warning.

After Crescendo activates, if you still have not updated, the blocks that you submit will be considered invalid.
