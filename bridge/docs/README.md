## Stratum Bridge Beta

This Stratum Bridge is currently in BETA. Support is available in the Kaspa Discord’s [#mining-and-hardware](https://discord.com/channels/599153230659846165/910178666099646584) channel.

For bug reports or feature request, please open an issue at [`kaspanet/rusty-kaspa` issues](https://github.com/kaspanet/rusty-kaspa/issues) and prefix your issue title with `[Bridge]`.

This repository contains a standalone Stratum bridge binary at:

`bridge`

The bridge can run against:

- **External** node (you run `kaspad` yourself)
- **In-process** node (the bridge starts `kaspad` in the same process)


### Default config / ports

The sample configuration file is:

`bridge/config.yaml`

When running from the repository root, pass the config path explicitly:

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

### Internal CPU miner (feature-gated)

The internal CPU miner is a **compile-time feature**.

Build:

```bash
cargo build -p kaspa-stratum-bridge --release --features rkstratum_cpu_miner
```

Run (external node mode + internal CPU miner enabled):

```bash
cargo run -p kaspa-stratum-bridge --release --features rkstratum_cpu_miner --bin stratum-bridge -- --config bridge/config.yaml --node-mode external --internal-cpu-miner --internal-cpu-miner-address kaspa:YOUR_WALLET_ADDRESS --internal-cpu-miner-threads 1
```

### Running two bridges at once (two dashboards)

If you run **two `stratum-bridge` processes** simultaneously (e.g. one in-process and one external),
they **cannot share the same**:
- `web_port` (dashboard)
- any Stratum ports
- any per-instance Prometheus ports

Recommended setup:
- **In-process bridge**: run normally with `--config config.yaml` (uses `web_port: ":3030"` and the configured instance ports)
- **External bridge**: do **not** reuse the same instance ports; instead, run a single custom Stratum instance on a different port and set a different web port.

Example (external bridge on `:3031` + Stratum `:16120`):

```bash
cargo run -p kaspa-stratum-bridge --release --features rkstratum_cpu_miner --bin stratum-bridge -- --config config.yaml --web-port :3031 --node-mode external --kaspad-address 127.0.0.1:16210 --instance "port=:16120,diff=1" --internal-cpu-miner --internal-cpu-miner-address "kaspatest:address" --internal-cpu-miner-threads 1
```

Open:
- `http://127.0.0.1:3030/` for the in-process bridge
- `http://127.0.0.1:3031/` for the external bridge

### Run (external node)

Terminal A (node):

```bash
cargo run --release --bin kaspad -- --utxoindex --rpclisten=127.0.0.1:16110 --rpclisten-borsh=127.0.0.1:17110 --rpclisten-json=127.0.0.1:18110
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

- **Pool URL:** `<your_pc_IPv4>:<stratum_port>` (e.g. `192.168.1.10:5555`)
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
