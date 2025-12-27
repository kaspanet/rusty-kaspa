## Stratum Bridge

This repository contains a standalone Stratum bridge binary at:

`bridge`

The bridge can run against:

- **External** node (you run `kaspad` yourself)
- **In-process** node (the bridge starts `kaspad` in the same process)

The bridge no longer supports spawning `kaspad` as a subprocess.

### Default config / ports

The sample configuration file is:

`bridge/config.yaml`

When running from the repository root, pass this full relative path via `--config`.

By default it exposes these Stratum ports:

- `:5555`
- `:5556`
- `:5557`
- `:5558`

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
cargo run -p kaspa-stratum-bridge --release --bin stratum-bridge -- --config bridge/config.yaml --node-mode inprocess --node-args="--utxoindex --rpclisten=127.0.0.1:16110 --rpclisten-borsh=127.0.0.1:17110"
```

**Note:** If you already have a `kaspad` running, in-process mode may fail with a DB lock error (RocksDB `meta/LOCK`). Either stop the other `kaspad` or run in-process with a separate app directory, e.g. add to `--node-args`:

```text
--appdir=E:\\rusty-kaspa\\tmp-kaspad-inprocess
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
