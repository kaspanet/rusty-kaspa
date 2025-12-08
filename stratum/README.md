# Kaspa Stratum Mining Protocol Server

This module provides a native Stratum protocol implementation for the Kaspa node, allowing miners to connect directly to the node without requiring a separate pool server.

## Status

✅ **Complete**
- Protocol message types and parsing
- Client connection handler
- Server implementation
- Integration with MiningManager
- **Block submission handling** - Full implementation with PoW validation, block reconstruction, and consensus submission
- **Job distribution** - Automatic job distribution loop with per-client difficulty support
- **Integration with kaspad** - Fully integrated into kaspad daemon, enabled via `--stratum-enabled` flag
- Variable difficulty (vardiff) - Automatic difficulty adjustment per miner
- Duplicate share detection
- Rate limiting and connection management

## Architecture

### Components

1. **Protocol (`protocol.rs`)**
   - Stratum request/response message types
   - JSON-RPC message parsing
   - Protocol helpers

2. **Client Handler (`client.rs`)**
   - Manages individual miner connections
   - Handles subscribe, authorize, and submit requests
   - Manages miner state and job tracking

3. **Server (`server.rs`)**
   - TCP server for accepting miner connections
   - Job distribution to connected miners
   - Integration with MiningManager for block templates

4. **Error Handling (`error.rs`)**
   - Stratum-specific error types
   - Error code mapping

## Protocol Support

The implementation supports:
- `mining.subscribe` - Miner subscription
- `mining.authorize` - Worker authorization
- `mining.submit` - Share/block submission
- `mining.set_difficulty` - Difficulty notifications
- `mining.notify` - New job notifications

## Configuration

The Stratum server can be configured via `StratumConfig`:

```rust
StratumConfig {
    listen_address: "0.0.0.0".to_string(),
    listen_port: 3333,
    default_difficulty: 1.0,
    enabled: false,  // Must be explicitly enabled
}
```

## Integration

To integrate into kaspad:

1. Add stratum dependency to kaspad's Cargo.toml
2. Initialize StratumServer in kaspad main
3. Start server when enabled
4. Pass ConsensusProxy and MiningManagerProxy

## Features

✅ **Implemented Features**
- Block submission with PoW validation against network difficulty
- Block reconstruction with nonce and consensus submission
- Automatic job distribution to all connected miners
- Per-client variable difficulty (vardiff) with power-of-2 clamping for ASIC compatibility
- Duplicate share detection to prevent double-counting
- Rate limiting and connection management
- Full integration with kaspad consensus and mining managers

## Future Enhancements

1. **Job Distribution Optimization**
   - Subscribe to new block template notifications (currently polls every 10 seconds)
   - More efficient job distribution for large numbers of miners

2. **Testing**
   - Unit tests for protocol parsing
   - Integration tests with test miners
   - Performance testing with multiple ASICs

3. **Documentation**
   - API documentation
   - Advanced configuration guide
   - Miner setup instructions for various ASIC models

## Safety

The Stratum server is designed to be:
- **Optional**: Disabled by default, must be explicitly enabled
- **Non-intrusive**: Does not modify consensus or block validation
- **Isolated**: Separate TCP server, independent from RPC
- **Backward compatible**: Does not affect existing node functionality

## References

- Based on the existing TypeScript/Bun stratum implementation in `pool/src/stratum/`
- Follows EthereumStratum/1.0.0 protocol specification
- Compatible with Bitmain/GodMiner encoding format

