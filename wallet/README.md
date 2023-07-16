
# Kaspa Rust Wallet

## Prerequisites

* rust 1.64.0+
* wasm-pack 0.10.3+ https://rustwasm.github.io/wasm-pack/installer/
* basic-http-server `cargo install basic-http-server`
(alternatively you can use your favorite flavor of http server, just make sure to match the ports in this example)

## Basic Operations

(this section will be updated later, it is intended for development)

After starting the wallet shell (native of WASM) and starting a local rusty-kaspa Kaspad node (with `--testnet` and `--utxoindex`), you should perform the following actions:
```
network testnet
server localhost
connect
create
```

- `network` configures the network type (testnet or mainnet), this also helps the system determine default RPC ports
- `server` configures the server address for wRPC Borsh connection
- `connect [<address>]` connects to the given server (`network` and `server` are used to determine the desirable connection endpoint if the address is not specified)
- `create` without arguments (on a new installation) will create a local wallet

At the end, you will get a mnemonic;  preserve that in case you need to reset the wallet storage at a later date.

If receiving a lot of transactions, you can use `mute` and `track <type>` commands to mute and toggle specicic types of notifications.

Please use `help` to get a complete list of commands.