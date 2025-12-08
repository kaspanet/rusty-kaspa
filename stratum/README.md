# Kaspa Stratum Mining Protocol Server

This module provides a native Stratum protocol implementation for the Kaspa node, allowing miners to connect directly to the node without requiring a separate pool server.

## Status

✅ **Basic Structure Complete**
- Protocol message types and parsing
- Client connection handler
- Server implementation skeleton
- Integration with MiningManager

🚧 **In Progress**
- Block submission handling
- Job distribution improvements
- Integration with kaspad main application

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

## Next Steps

1. **Complete Block Submission**
   - Reconstruct block header with nonce
   - Validate share difficulty
   - Submit valid blocks to consensus

2. **Job Distribution**
   - Subscribe to new block template notifications
   - Efficiently distribute jobs to all miners
   - Handle job expiration

3. **Difficulty Management**
   - Variable difficulty (vardiff) support
   - Per-miner difficulty adjustment
   - Share difficulty validation

4. **Testing**
   - Unit tests for protocol parsing
   - Integration tests with test miners
   - Performance testing

5. **Documentation**
   - API documentation
   - Configuration guide
   - Miner setup instructions

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

