# `KOS`

An integrated desktop application that provides a local Kaspa node instance, a CPU miner (for testnet)
and the Wallet subsystem based on the Rusty Kaspa project.  This application is written in Rust and integrates
the Rusty Kaspa framework.

This application is compatible with Windows, MacOS and Linux desktop environments.

Please note that this project is comprised of 3 top-level components:
- `/kos` - desktop application environment (can run as a dekstop application)
- `/cli` - native terminal environment (can run from a command line)
- `/wallet/wasm` - web application (can run in the browser)

All three components listed above are the same application written in Rust, the only
difference is that the `cli` and `wallet` can not run `kaspad` (but can connect remotely to it).
Also, `cli` and `kos` store all wallet data on the filesystem and are interchangeable,
while the `web wallet` stores the wallet data in your browser (associated and locked by your browser
to the domain name where the wallet is running).

## Dependencies

- cargo nw: `cargo install cargo-nw` (`cargo nw --help` for more info)

Cargo NW is a tool for running and deploying NWJS applications.

Please make sure that you have the latest Rust version by running `rustup update` and the latest
`wasm-pack` by running `cargo install wasm-pack`.

If you have not previously setup Rusty Kaspa development environment, please follow the README
in the root of this project.

## Repositories

To use KOS on the testnet, you need to clone rusty-kaspa and cpu-miner repositories as follows.

```
mkdir kaspa
cd kaspa
git clone -b kos https://github.com/aspectron/rusty-kaspa
git clone https://github.com/aspectron/kaspa-cpu-miner
```
(please note that you are cloning `kos` branch from `aspectron/rusty-kaspa` repository until KOS is merged into `kaspanet/master`)

## Building

Please note that to run this application in the development environment
you need to build Rusty Kaspa and CPU Miner. You can do that as follows:

```
cd rusty-kaspa
cargo build --bin kaspad --release
cd ../kaspa-cpu-miner
cargo build --release
cd ../rusty-kaspa/kos
```


### Release
```
cd rusty-kaspa/kos
./build
```
### Debug
```
cd rusty-kaspa/kos
./build --dev
```

## Running

- regular: `cargo nw run`
- with NWJS SDK: `cargo nw run --sdk`
- using local cargo-nw: `cargo run -- nw ../rusty-kaspa/kos run --sdk`

The `--sdk` option allows you to bring up the development console which
gives you access to application log outputs (using `log_info!` and related macros).


## KOS and CLI

NOTE: `/kos` and `/cli` are practically identical. KOS runs in a desktop environment 
powered by NWJS and automated via `cargo-nw`. CLI runs in a command-line environment.
KOS is capable of running and managing Rusty Kaspa node (`kaspad`) as a child process as 
well as (during initial alpha stages) a CPU miner.

KOS will auto-detect Kaspa node and CPU miner applications by looking at `../target/release/kaspad` and `../../kaspa-cpu-miner/target/release/kaspa-cpu-miner`

You can also use `node select` and `miner select` commands to select release or debug versions or supply a custom path as a 3rd argument to these commands.

## Basic Guide

Once you power up KOS you can do the following:

Before you do anything else, enter: `network testnet-10` - specify that you want to interface with `testnet-10`.

`node start` - start the kaspa node; you will need to wait for the node to sync

`node mute` - toggles node log output

`wallet create` - create a local wallet;  You can create multiple wallets by specifying a name after the create command. For example: `wallet create alpha` will create a wallet `alpha` which can later be opened with `open alpha`.  The default name for the wallet is `kaspa`.  Each named wallet is stored in a separate wallet file.  (you can not currently rename the wallet once created)

`account create bip32` - creates a new account in the currently opened wallet.  You can give a name to the account by supplying it during creation as follows: `account create bip32 personal` or `account create bip32 business`.  This will create an account named `personal` or `business`.  Account names can be later used as shorthand when transferring funds between accounts or selecting them.

Once you are synced and have the miner operational, make sure you have a wallet open with a selected account and use `miner start`.  This will launch the miner and start mining to your selected account.  If your hashrate is in Kh/s, it may take a while to find a block.

Once you have received some TKAS, you can test sending it by doing `transfer p 10`. The letter `p` is for `personal` - when using account names you can use first set of letters of the account name or it's id. If more than one account matches your supplied prefix, you will be asked to be more specific.

Use `list` to see your accounts and their balances.

You can click on any address to copy it to the clipboard

Use `mute` to toggle visibility of internal framework events.  Applications integrating with this framework will receive these events and will be able to update UI accordingly.

Use `send <address> 10` to send funds to someone else.

Use `guide` for an internal guide that provides additional information about supported commands.

