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
difference is that the `cli` and `wallet` can not run `kaspad` (but can conned remotely to them).
Also, `cli` and `kos` store all wallet data on your filesystem and are interchangeable,
while the `wallet` stores the wallet data in your browser (associated and locked by your browser
to the domain name where the wallet is running).

## Dependencies

- cargo nw: `cargo install cargo-nw` (`cargo nw --help` for more info)

## Repositories

To use kaspa-os on the testnet, you need to clone rusty-kaspa and cpu-miner repositories as follows.

```
mkdir kaspa
cd kaspa
git clone https://github.com/kaspanet/rusty-kaspa
git clone https://github.com/aspectron/kaspa-cpu-miner
```

## Building

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

## Redistributables

### Prerequisites
- Windows: [InnoSetup 6.2.2](https://jrsoftware.org/isdl.php) (or higher)
- Linux: SnapCraft (please see below)

### Building

- Windows Installer: `cargo nw build innosetup`
- MacOS DMG: `cargo nw build dmg`
- Linux SnapCraft: `cargo nw build snap`
- Zip Archive: `cargo nw build archive`

For additional build options please refer to the [Cargo NW manual](https://cargo-nw.aspectron.org/command-line/build.html)

### Linux SnapCraft

Snap redistributable target requires SnapCraft to be installed on the linux operating system.
```
sudo apt install libssl-dev
sudo snap install snapcraft
sudo snap install lxd
sudo adduser <username> lxd
sudo service snap.lxd.daemon restart
# you may need to restart the system
```

When creating SNAP files, to install them locally you need to run:

```
# when building with `strict` containment
snap install --dangerous <yourfile>.app
# when building with `clssic` containment
snap install --dangerous --classic <yourfile>.app
```

## KOS and CLI

NOTE: `/kos` and `/cli` are practically identical. KOS runs in a desktop environment 
powered by NWJS and automated via `cargo-nw`. CLI runs in a command-line environment.
KOS is capable of running and managing `rusty-kaspa` node as a child process as 
well as (during initial alpha stages) a CPU miner.

- To run Rusty Kaspa, build rusty-kaspa by running `cargo build --bin kaspad --release`
- To run CPU Miner, clone https://github.com/aspectron/kaspa-cpu-miner as a sibling folder to `rusty-kaspa` and run `cargo build --release` inside of `kaspa-cpu-miner`

KOS will auto-detect these applications by looking at `../target/release/kaspad` and `../../kaspa-cpu-miner/target/release/kaspa-cpu-miner`

You can also use `node select` and `miner select` commands to select release or debug versions or supply a custom path as a 3rd argument to these commands.

## Installers

Please note that redistributable installers 

## Basic Guide

Once you power up KOS you can do the following:

`network testnet-10` - specify that you want to interface with testnet-10

`node start` - start the kaspa node; you will need to wait for the node to sync

`node mute` - toggles node log output

`wallet create` - create a local wallet;  You can create multiple wallets by specifying a name after the create command. For example: `wallet create alpha` will create a wallet `alpha` which can later be opened with `open alpha`.  The default name for the wallet is `kaspa`.  Each named wallet is stored in a separate wallet file.  (you can not currently rename the wallet once created)

`account create` - creates a new account in the currently opened wallet.  You can give a name to the account by supplying it during creation as follows: `account create personal` or `account create business`.  This will create an account named `personal` or `business`.  Account names can be later used as shorthand when transferring funds between them.

Once you are synced and have the miner operational, make sure you have a wallet open with a sepected account and use `miner start`.  This will launch the miner and start mining to your selected account.  If your hashrate is in Kh/s, it may take a while to find a block.

Once you have received some TKAS, you can test sending it by doing `transfer p 10`. The letter `p` is for `personal` - when using account names you can use first set of letters of the account name or it's id. If more than one account matches your supplied prefix, you will be asked to be more specific.

You can type `list` to see your accounts.

Click on any address to copy it to the clipboard

Use `send <address> 10` to send funds to someone else.

Use `mute` to toggle visibility of internal events.