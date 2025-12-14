# Stratum Bridge Integration into Kaspad

## Overview

The Stratum Bridge has been fully integrated into the rusty-kaspa node daemon (kaspad) as a native service. The integration preserves all ASIC-specific handling and multi-miner support architecture.

## Architecture Preservation

The integration maintains the bridge's sophisticated multi-ASIC architecture:

### 1. Auto-Detection System ✅
- **Location**: `default_client::handle_subscribe` → `ClientHandler::assign_extranonce_for_miner`
- **How it works**: Detects miner type from `remote_app` user-agent string
- **Supported miners**: IceRiver, Bitmain (GodMiner/Antminer), BzMiner, Goldshell
- **Integration**: Properly wired through handler overrides in `listen_and_serve`

### 2. Per-Client Extranonce Assignment ✅
- **Bitmain**: extranonce_size = 0 (no extranonce)
- **IceRiver/BzMiner/Goldshell**: extranonce_size = 2 (2-byte pool, wraps at 65535)
- **Integration**: `extranonce_size: 0` in config enables auto-detection per client

### 3. Different Message Formats ✅
- **Bitmain subscribe**: `[null, extranonce, extranonce2_size]` (extranonce in response)
- **IceRiver/BzMiner subscribe**: `[true, "EthereumStratum/1.0.0"]` (extranonce sent separately)
- **IceRiver notifications**: Minimal format (no id/jsonrpc fields)
- **Integration**: Handled in `handle_subscribe` and `send_extranonce` based on detected type

### 4. Critical Message Ordering (IceRiver) ✅
**Required sequence**:
1. `mining.authorize` response
2. `mining.set_extranonce` (if enabled)
3. `mining.set_difficulty`
4. `mining.notify` (job)

**Integration**: Preserved in `handle_authorize` which ensures sequential sending

### 5. Different Job Parameter Formats ✅
- **IceRiver**: 80-char hex string (`generate_iceriver_job_params`)
- **BzMiner**: Big-endian hex string (`generate_large_job_params`)
- **Bitmain**: Array format (`generate_job_header`)
- **Integration**: Handled in `ClientHandler::new_block_available` based on detected type

### 6. Immediate Job Sending ✅
- After authorization, job sent immediately via `send_immediate_job_to_client`
- Does not wait for polling loop
- **Integration**: `authorize` handler receives both `client_handler` and `kaspa_api` for immediate sending

### 7. Per-Client MiningState ✅
- Each client has isolated state with own job tracking
- Prevents job ID collisions
- **Integration**: Created per-client in `StratumListener` connection handling

### 8. Variable Difficulty ✅
- Per-client adjustment with power-of-2 clamping
- Automatic adjustment to target shares_per_min
- **Integration**: Enabled via `var_diff` config option, handled by `ShareHandler`

## Integration Points

### Handler Setup
The `listen_and_serve` function properly sets up all handlers:

```rust
// Subscribe handler - enables auto-detection
handlers.insert("mining.subscribe", subscribe_handler_with_client_handler);

// Authorize handler - enables immediate job sending
handlers.insert("mining.authorize", authorize_handler_with_client_handler_and_api);

// Submit handler - enables share validation and block submission
handlers.insert("mining.submit", submit_handler_with_share_handler_and_api);
```

### Block Template Updates
- Uses notification-based updates when concrete `KaspaApi` is provided
- Falls back to polling if notifications unavailable
- **Integration**: `listen_and_serve` called with `Some(kaspa_api)` for notification support

### Connection Lifecycle
- `on_connect`: Registers client in `ClientHandler`, assigns ID
- `on_disconnect`: Removes client, cleans up state
- **Integration**: Properly wired through `StratumListenerConfig`

## Command-Line Interface

All bridge functionality is accessible via kaspad flags:

```bash
# Enable bridge
kaspad --stratum-bridge-enabled

# Configure port
kaspad --stratum-bridge-enabled --stratum-bridge-port 5555

# Full configuration
kaspad \
  --stratum-bridge-enabled \
  --stratum-bridge-port 5555 \
  --stratum-bridge-min-diff 8192 \
  --stratum-bridge-var-diff true \
  --stratum-bridge-shares-per-min 30 \
  --stratum-bridge-prom-port 2114
```

## Verification

The integration has been verified to:
- ✅ Build successfully
- ✅ Preserve all ASIC-specific logic
- ✅ Maintain proper handler wiring
- ✅ Support notification-based block updates
- ✅ Enable per-client auto-detection
- ✅ Handle all miner types correctly

## Key Files

- **Integration**: `kaspad/src/daemon.rs` (lines 662-744)
- **Bridge entry**: `stratum-bridge/src/stratum_server.rs::listen_and_serve`
- **ASIC detection**: `stratum-bridge/src/client_handler.rs::assign_extranonce_for_miner`
- **Handler setup**: `stratum-bridge/src/stratum_server.rs` (lines 90-141)
- **Message handling**: `stratum-bridge/src/default_client.rs`

## Notes

- The bridge runs as an `AsyncService` within kaspad's runtime
- It connects to the local node's gRPC server automatically
- All logging goes through kaspad's logging system
- The bridge shuts down cleanly with the node
- No separate configuration file needed - everything via command-line flags

