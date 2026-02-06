## Stratum Bridge Beta

This Stratum Bridge is currently in BETA. Support is available in the Kaspa Discord’s [#mining-and-hardware](https://discord.com/channels/599153230659846165/910178666099646584) channel.

For bug reports or feature request, please open an issue at https://github.com/kaspanet/rusty-kaspa/issues and prefix your issue title with [Bridge].

This repository contains a standalone Stratum bridge binary at:

`bridge`

The bridge can run against:

- **External** node (you run `kaspad` yourself)
- **In-process** node (the bridge starts `kaspad` in the same process)


### Default config / ports

The sample configuration file is:

`bridge/config.yaml`

When running from the repository root, pass this full relative path via `--config`.

By default it exposes these Stratum ports:

- `:5555`
- `:5556`
- `:5557`
- `:5558`

### CLI Help

For detailed command-line options:

```bash
cargo run --release --bin stratum-bridge -- --help
```

This will show all available bridge options and guidance for kaspad arguments.

### Run (external node)

Terminal A (node):

```bash
cargo run --release --bin kaspad -- --utxoindex --rpclisten=127.0.0.1:16110 --rpclisten-borsh=127.0.0.1:17110
```

Terminal B (bridge):

```bash
cargo run -p kaspa-stratum-bridge --release --bin stratum-bridge -- --config bridge/config.yaml --node-mode external
```

### Run (in-process node)

```bash
cargo run -p kaspa-stratum-bridge --release --bin stratum-bridge -- --config bridge/config.yaml --node-mode inprocess -- --utxoindex --rpclisten=127.0.0.1:16110 --rpclisten-borsh=127.0.0.1:17110
```

**Important:** Use `--` separator before kaspad arguments. Arguments starting with hyphens must come after the `--` separator.

**Examples:**
```bash
# ✓ Correct - bridge args first, then --, then kaspad args
cargo run --release --bin stratum-bridge -- --config config.yaml --node-mode inprocess -- --utxoindex --rpclisten=127.0.0.1:16110

# ✗ Incorrect - will show error message
cargo run --release --bin stratum-bridge -- --rpclisten=127.0.0.1:16110 --config config.yaml --node-mode inprocess
# Error: tip: to pass '--rpclisten' as a value, use '-- --rpclisten'
```

**Note:** In-process mode uses a separate app directory by default to avoid RocksDB lock conflicts with an existing `kaspad`.

If you want to override it, pass `--appdir` to the bridge (before the `--` separator):

```bash
cargo run --release --bin stratum-bridge -- --config bridge/config.yaml --node-mode inprocess --appdir "C:\path\to\custom\datadir" -- --utxoindex --rpclisten=127.0.0.1:16110
```

### Miner / ASIC connection

- **Pool URL:** `<your_pc_ip>:5555` (or whichever `stratum_port` you configured)
- **Username / wallet:** `kaspa:YOUR_WALLET_ADDRESS.WORKERNAME`

To verify connectivity on Windows:

```powershell
netstat -ano | findstr :5555
```

To see detailed miner connection / job logs:

```powershell
$env:RUST_LOG="info,kaspa_stratum_bridge=debug"
```

On Windows, Ctrl+C may show `STATUS_CONTROL_C_EXIT` which is expected.
