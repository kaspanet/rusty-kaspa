# Testnet 11

*Testnet 11* is the network where we will launch the first public 10BPS Kaspa experiment. The goal of this document is to provide a quick guide for anyone who wants to participate.

In the future testnet 11 will act as a staging zone for all sorts of crazy experiments, allowing us to stress test various approaches and ideas on a global scale with the participation of the community. The approaches we decide to adopt will then be stress tested for longer periods on testnet 10 before being incorporated into the mainnet.

## The first experiment

The goal of the first experiment is to stress load the network in terms of block rates and transaction rates, in varying network conditions. In particular, we are curious to see how the network responds to *mildly* varying hash rates, so we encourage users to freely join and leave the network, or set up several nodes. **However**, we kindly ask users not to abuse the hashrate and **only use the included CPU miner**. Turning on a GPU rig (or, god forbid, a KS02) to increase the global hashrate by 10000% at once might be amusing, but not very productive.

## Overview

On the software side, participating requires three components:
1. *kaspad* - the Kaspa client
2.  *kaspaminer* - the Kaspa miner (duh)
3.  *Rothschild* - a transaction generator

The Rothschild tool is used to create a wallet, and once the wallet has some funds within, Rothschild will continuously create transactions from that wallet back to that wallet at the prescribed rate.

The Rothschild wallet could be funded by either mining to it directly (either for a short period or continuously) or by asking other experiment participants for some funds (e.g. on the Discord \#testnet channel).

## How to participate

The only prerequisite is running a node that's connected to Testnet 11. Other than that you produce blocks by running a miner, produce transactions by running Rothschild, or both.

Since we want the test condition to be as close as possible to organic, we encourage users to diversify their roles, and the hardware they use to participate.

The venue for discussing and monitoring the experiment will be the \#testnet channel on Discord. We encourage participants to describe the experience in general, and also tell us what hardware they are using and how well handles the load.

The minimal hardware requirements are 16GB or RAM, preferably a CPU with at least 8 cores, and an SSD drive with at least 50GB of free space (the 100GB for a safety margin is preferable).

## Setup Instructions

Testnet11 uses a dedicated P2P port (16311) so that nodes from the usual tesnet (10) do not automatically attempt connecting to it.

We reiterate that only the included miner should be used to maintain a level playing field.

To set things up:
0. Download and extract the rusty-kaspa binaries <add URL>. Alternatively, you can compile it from source yourself by following the instructions [here](../blob/master/README.md)
0. run the node with utxoindex enabled cargo run --bin kaspad --release -- --testnet --netsuffix=11 --utxoindex
1. run Rothschild (the tx generator tool): cargo run --bin rothschild --release --
2. this will give you a public address/private key pair. Then set up a miner mining to the pub address  (with go the command is: ./kaspaminer --testnet --miningaddr <pub address> --target-blocks-per-second=0) 
3. after an ~20 minutes (when your miner has enough funds) re-run rothschild with the private key cargo run --bin rothschild --release -- --private-key <priv-key> -t=50. 
4. we recommend running with a TPS value of 50-100 so that tx generation spreads over nodes in the net. Having 20-30 people running this setting will create a nice distributed load approaching max capacity (which is approx 3000 standard txs per second).  

Node requirements are 16GB RAM (might be reduced to 8GB but no guarantee yet), preferably at least 8 cores and at least 50GB SSD (100GB should give a safe margin).
