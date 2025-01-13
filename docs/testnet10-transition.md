# Kaspa Testnet 10 (TN10) – Crescendo Hardfork Node Setup Guide

Kaspa is about to take a significant leap with the **Crescendo Hardfork**, as detailed in [KIP14](https://github.com/kaspanet/kips/blob/master/kip-0014.md), transitioning from 1 to 10 blocks per second. To ensure a stable rollout, **Testnet 10 (TN10)** will first undergo this shift on approximately **March 6, 2025, 18:30 UTC**. By running TN10 and providing feedback, you help prepare for a smooth mainnet upgrade, tentatively planned for the end of April or early May.

---

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

While the minimum specs suffice to sync and maintain a TN10 node with the accelerated 10 bps, increasing CPU cores, RAM, storage, and bandwidth allows your node to serve as a stronger focal point on the network. This leads to faster initial block download (IBD) for peers syncing from your node and provides more leeway for future storage growth and optimization.


---

## 1. Install & Run Your TN10 Node

1. **Obtain the latest Kaspa binaries**  
   Download and extract the latest [official release](https://github.com/kaspanet/rusty-kaspa/releases/), or build from the `master` branch by following the instructions in the project README.

2. **Launch the Node**  
   While TN10 is the default netsuffix, specifying it explicitly is recommended:

   ```
   kaspad --testnet --netsuffix=10 --utxoindex
   ```

   *(If running from source code:)*  
   ```
   cargo run --bin kaspad --release -- --testnet --netsuffix=10 --utxoindex
   ```

Leave this process running. Closing it will stop your node.

- **Advanced Command-Line Options**:
  - `--rpclisten=0.0.0.0` to listen for RPC connections on all network interfaces (public RPC).
  - `--rpclisten-borsh` for local borsh RPC access from the `kaspa-cli` binary.
  - `--unsaferpc` for allowing P2P peer query and management via RPC (recommended to use only if **not** exposing RPC publicly).
  - `--perf-metrics --loglevel=info,kaspad_lib::daemon=debug,kaspa_mining::monitor=debug` for detailed performance logs.
  - `--loglevel=kaspa_grpc_server=warn` for suppressing most RPC connect/disconnect log reports.
  - `--ram-scale=3.0` for increasing cache size threefold (relevant for utilizing large RAM; can be set between 0.1 and 10).

---

## 2. Generate Transactions with Rothschild

1. **Create a Wallet**
  ```
  rothschild
  ```

   This outputs a private key and a public address. Fund your wallet by mining to it or obtaining test coins from other TN10 participants.

2. **Broadcast Transactions**  
  ```
  rothschild --private-key <your-private-key> -t=10
  ```

   Replace <your-private-key> with the key from step 1. The `-t=10` flag sets your transaction rate to 10 TPS (feel free to try different rates, but keep it below 50 TPS).

---

## 3. Mining on TN10

1. **Download the Miner**  
   Use the latest Kaspa CPU miner [release](https://github.com/elichai/kaspa-miner/releases) which supports TN10.

2. **Start Mining**  
  ```
  kaspa-miner --testnet --mining-address <your-address> -p 16210 -t 1
  ```

   Replace <your-address> with your TN10 address (e.g., from Rothschild) if you want to mine and generate transactions simultaneously.

---

## Summary & Next Steps

- **Node Sync:**  
  `kaspad --testnet --netsuffix=10 --utxoindex`
- **Transaction Generation:**  
  `rothschild --private-key <your-private-key> -t=10`
- **Mining:**  
  `kaspa-miner --testnet --mining-address <your-address> -p 16210 -t 1`  

By participating in TN10, you help stress-test the Crescendo Hardfork environment and prepare for a robust mainnet upgrade in end of April / early May. Share any challenges or successes in the #testnet Discord channel, and thank you for supporting Kaspa’s continued evolution.

