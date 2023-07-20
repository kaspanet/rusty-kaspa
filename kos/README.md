# `kaspa-os`

An integrated desktop application that provides a local Kaspa node instance, a CPU miner (for testnet)
and the Wallet subsystem based on the Rusty Kaspa project.  This application is written in Rust and integrates
the Rusty Kaspa framework.

This application is compatible with Windows, MacOS and Linux desktop environments.

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
