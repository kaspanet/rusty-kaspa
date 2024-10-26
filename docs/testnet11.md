# Testnet 11

_Testnet 11_ is the network where we will launch the first public 10BPS Kaspa experiment. This document aims to provide a quick guide for anyone who wants to participate.

In the future, Testnet 11 will act as a staging zone for various experiments, allowing us to stress test different approaches and ideas on a global scale with community participation. The approaches we decide to adopt will be stress-tested for longer periods on Testnet 10 before being incorporated into the mainnet.

## Overview

On the software side, participating requires three components:

1. **_kaspad_** - the Kaspa client
2. **_kaspa-miner_** - the Kaspa CPU miner
3. **_Rothschild_** - a transaction generator

The Rothschild tool is used to create a wallet, and once the wallet has some coins, Rothschild will continuously create transactions from that wallet back to itself at the prescribed rate.

The Rothschild wallet can be funded either by mining to it directly (for a short period or continuously) or by asking other experiment participants for some coins (e.g., on the Discord \#testnet channel).

## How to Participate

The only prerequisite is running a node connected to Testnet 11. From there, you can either produce blocks by running a miner, generate transactions by running Rothschild, or do both.

To simulate a real-world scenario as closely as possible, we encourage participants to diversify their roles and the hardware they use.

The venue for discussing and monitoring the experiment will be the \#testnet channel on Discord. We encourage participants to share their experience and hardware specifications, along with how well their systems handle the load.

### Recommended Hardware Requirements

- 16GB of RAM (8GB may work with the `--ram-scale=0.6` parameter)
- CPU with at least 8 cores
- SSD with at least 250GB of free space (300GB for a safety margin)

## Setup Instructions

Testnet 11 uses a dedicated P2P port (16311), ensuring that nodes from other testnets (like Testnet 10) don’t automatically connect to it.

We emphasize that **only the included miner should be used** to maintain fairness.

### Step 1: Set Up a Node

1. Download and extract the [rusty-kaspa binaries](https://github.com/kaspanet/rusty-kaspa/releases). Alternatively, you can compile it from source by following [these instructions](https://github.com/kaspanet/rusty-kaspa/blob/master/README.md). This guide assumes you are using the precompiled binaries. If compiling locally, adjust commands like `<program> <arguments>` to `cargo run --bin <program> --release -- <arguments>`.

   All commands below should be run from the directory where the binaries were extracted.

2. Start the `kaspad` client with `utxoindex` enabled:

   ```
   kaspad --testnet --netsuffix=11 --utxoindex
   ```

   Be sure not to forget the `--netsuffix=11` flag, as omitting it will connect your node to the mainnet or the default 1 BPS testnet. If you compiled the code yourself, run:

   ```
   cargo run --bin kaspad --release -- --testnet --netsuffix=11 --utxoindex
   ```

   Keep this window open, as closing it will stop the node.

### Step 2: Set Up a Rothschild Wallet

1. Run `rothschild` to generate a wallet.
2. The output will provide a private key and a public address. For example:

   ```
   2023-06-25 18:00:58.677+00:00 [INFO ] Connected to RPC
   2023-06-25 18:00:58.677+00:00 [INFO ] Generated private key aa1c554386218eb28c4bsf6a02e5943799cf951dac7301324d88dec2d0119fce and address kaspatest:qzlpwt49f0useql6w0tzpnf8k2symdv5tu2x2pe9r9nvngw8mvx57q0tt9lr5. Send some funds to this address and rerun rothschild with `--private-key aa1c554386218eb28c4bsf6a02e5943799cf951dac7301324d88dec2d0119fce`
   ```

   Here, the private key is `aa1c554386218eb28c4bsf6a02e5943799cf951dac7301324d88dec2d0119fce`, and the address is `kaspatest:qzlpwt49f0useql6w0tzpnf8k2symdv5tu2x2pe9r9nvngw8mvx57q0tt9lr5`.

3. Add some coins to the wallet. You can do this by mining to that wallet (see below) or asking other participants to send coins to your public address in the \#testnet Discord channel.

4. Once the wallet is funded, run Rothschild with the private key:

   ```
   rothschild --private-key <private-key> -t=50
   ```

   The `-t=50` parameter means Rothschild will attempt to broadcast 50 transactions per second. We encourage users to experiment with different transaction rates, but avoid going over 100 TPS to simulate organic usage.

### Step 3: Start Mining

Download `kaspa-miner` from the latest [Release](https://github.com/elichai/kaspa-miner/releases) and run it with the following flags (**this is the only miner that supports Testnet 11**):

```
kaspa-miner --testnet --mining-address <address> -p 16210 -t 1
```

If you plan to run Rothschild, replace `<address>` with the address of your Rothschild wallet. Wait for the wallet to accumulate coins (about 20 minutes assuming several participants). If you are mining without running Rothschild, you can use any address, like the example provided above.

Keep the Kaspad, Rothschild, and miner windows open while they are running. Closing them will stop their respective processes.
