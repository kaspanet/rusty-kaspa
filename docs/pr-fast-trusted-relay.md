# Fast Trusted Relay (FTR) Implementation

## Summary

**Note: This is a draft PR, published for testing visibility, and not considered stable!**

Introduces a **Fast Trusted Relay** (FTR) network protocol for Kaspa, enabling low-latency block propagation via a custom push based UDP transport protocol with Reed-Solomon Forward Error Correction (FEC). Designed for trusted operators (mining pools, infrastructure providers, or even just altruistic actors) to achieve minimal block transfer latency across geographically distributed nodes. Although this directly profits those that require low-latency block propagation, any fast relay overlay, also indirectly helps the global kaspa network reduce propagation. Further use-cases may include satellite communications, where RTTs can inherently become a limiting factor, or faster communication between nodes situated within one data-center. The application of the customizable FEC also facilitates adopting this protocol in potentially lossy network conditions.

Furthermore, Fast Relay networks are oftentimes exclusive services offered by enterprise for enterprise, which can easily compound, for example, pool advantages over individual solo miners, by offering an open source easily accessible Fast Relay option on the node, free for anyone to use, we limit such centralized advantages and also help facilitate more such overlays to establish themselves on the kaspa network.

The main observable changes are that logs will display the number of blocks propagated via the fast relay network, alongside the normal blocks added logs, as such:

```
2026-03-26 14:14:40.849+01:00 [INFO ] Accepted 9 blocks ...2ab529f31b82379f60d1799b3d72d06eac3ca77b29b1f8dfb8dde3ec7737a4ac via relay
2026-03-26 14:14:40.849+01:00 [INFO ] Accepted 5 blocks ...2afc49022f1c41890b182252b80844992afcac58deb1f2b6661e5d1049e17d4b via trusted relay
```

One key limitation of this protocol is that due to its push-based nature it cannot function in isolation, and should / must be used together with standard relay. This is quintessential for resolving protocol aspects such as orphan resolution, etc... Currently, adding a peer to the FTR network will automatically add the peer to the standard relay as well, in order to facilitate such operations.

Although this PR is a far-shot away from replacing our standard tcp relay, adopting and adapting such a custom udp protocol could potentially be an entry point for gathering data and experience for such a maneuver, many projects, such as solana's Turbine block propagation, already use highly customized and optimized udp transport layers, in order to reduce latency.

## Motivation

Standard P2P block relay (TCP-based with request-response semantics) introduces latency that compounds across multiple hops and during network congestion. For operations where latency directly impacts orphan rates and revenue, a dedicated push-based relay with erasure coding provides significant advantages:

- **Push-based propagation**: No round-trip handshakes per block
- **Loss tolerance**: FEC allows recovery even when packets are lost in lossy network conditions
- **Parallel processing**: Multi-threaded pipeline saturates bandwidth

One trade-off is that the FTR bypasses block signaling and propagates whole blocks. Furthermore, FEC increases the bandwidth usage, which quickly compounds with the number of peers in the network. As such, a large network bandwidth should be a prerequisite to run such a FTR network. That being said, facilitating 10-20 peers (which should easily be enough for a good globally dispersed network) on a 1Gbit or 10Gbit connection should easily be sustainable, As should a small handful of peers on even more limited bandwidth.

## Architecture

### Two-Plane Design

| Plane | Transport | Purpose |
|-------|-----------|---------|
| **Control** | TCP (port 16113) | Handshake, peer management, ready-state signaling |
| **Data** | UDP (port 16114) | Block fragments with FEC encoding |

### Data Plane Pipeline

```
UDP Socket
    ↓
Collector (packet ingestion)
    ↓ [route by fragment_index % verifiers]
Verifier (HMAC validation, deduplication)
    ├→ Reassembler (fragment pooling, FEC trigger)
    │       ↓
    │   Decoder (Reed-Solomon recovery, pool -> block)
    │       ↓
    │   recv_block() → Flow layer → Consensus
    │
    └→ Relay Worker (forward packet to other outbound peers)

Standard p2p flow
    ↓
Broadcaster (encode and fragment block, forward packets to outbound peers)
```

Note: The stack is designed upon crossbeam worker threads, and can easily be multithreaded in high load environments.

## Key Features

### Reed-Solomon FEC Encoding

- Default: k=16 data fragments, m=4 parity fragments per generation.
- Receiver needs only k of (k+m) fragments to recover data
- Tolerates ~20% packet loss.
- SIMD-accelerated via `reed-solomon-simd` crate (although actual benefits compared to other crates have not been benchmarked)

### HMAC-SHA256 Authentication

- Offers basic DDoS protection.
- Per-fragment MAC: `hmac(secret, header ‖ payload)`
- TCP handshake: `hmac(secret, nonce ‖ direction ‖ udp_port)`
- Shared secret across trusted peer group

### IBD-Aware Control

- UDP transport is designed to be toggleable
- Auto-disabled during Initial Block Download (IBD)
- Re-enabled when node considers itself synced
- Prevents relaying blocks node cannot validate

## New Crate: `kaspa-trusted-relay`

### Module Structure

```
protocol/trusted_relay/src/
├── fast_trusted_relay.rs    # Core forward facing FastTrustedRelay struct
├── params.rs                # TransportParams, FragmentationConfig
├── error.rs                 # RelayErrors (Not fully implemented, currently errors are only logged)
├── model/
│   ├── ftr_block.rs         # FtrBlock wire format
│   └── fragments.rs         # FragmentHeader (36 bytes)
├── codec/
│   ├── encoder.rs           # FragmentGenerator (with FEC encoding)
│   ├── decoder.rs           # Reed-Solomon decoding
│   └── buffers.rs           # BlockDecodeState (specialized buffers for reassembly pooling)
└── servers/
    ├── auth.rs              # TokenAuthenticator
    ├── peer_directory.rs    # PeerInfo, PeerDirectory (shared between UDP transport and TCP control)
    ├── tcp_control/
    │   ├── tcp.rs           # TCP listener, handshake
    │   ├── hub.rs           # Peer registry, tcp event routing
    │   └── peer.rs          # Per-peer control loop
    └── udp_transport/
        ├── runtime.rs       # UDP TransportRuntime lifecycle
        └── pipeline/        # Worker threads for the udp packets
            ├── collector.rs    # UDP recv threads (collects the packets)
            ├── verification.rs # Data integrity checks + HMAC verification + packet Dedup
            ├── reassembly/     # Block Fragment pooling and re-assembly
            ├── broadcast.rs    # Outbound Block Fragmentation + Encoding + Packet Sending
            └── relay.rs        # Packet Relay to other peers.
```

### Public API

```rust
impl FastTrustedRelay {
    pub fn new(params, frag_config, listen_addr, secret, incoming, outgoing) -> Self
    // Starts the udp transport
    pub async fn start_fast_relay(&self) -> bool
    // Stops the udp transport
    pub async fn stop_fast_relay(&self) -> bool
    // Fragments, Encodes and Broadcasts a full Block to FTR peers
    pub async fn broadcast_block(&self, hash: Hash, block: Arc<FtrBlock>) -> Result<(), String>
    // Listen / Awaits on blocks received via the FTR
    pub async fn recv_block(&self) -> (Hash, FtrBlock)
    // Shutdown the FTR
    pub async fn shutdown(&self)
    // Indicates if the udp transport is active.
    pub async fn is_udp_active(&self) -> bool
}
```

## Flow Integration

### New Flow: `HandleFastTrustedRelayFlow`

This is a new "routerless" flow added to p2p which listens on and handles incoming FTR blocks.

Located in `protocol/flows/src/fast_trusted_relay/flow.rs`:

- Implements `Flow` trait (routerless)
- Loops on `recv_block()`
- Validates block status, handles orphans
- Broadcasts accepted blocks via standard P2P `InvRelayBlock`
- Graceful shutdown via `Listener`

### FlowContext Integration

- Stores optional `FastTrustedRelay` instance
- Block logging distinguishes trusted relay source
- Spawns FTR flow if relay configured

## CLI Arguments

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `--trusted-relay-incoming` | `Vec<Address>` | `[]` | Peers to receive blocks from |
| `--trusted-relay-outgoing` | `Vec<Address>` | `[]` | Peers to send blocks to |
| `--trusted-relay-secret` | `String` | Required | Shared HMAC secret |
| `--fec-data-blocks` | `usize` | `16` | FEC k parameter (4-128) |
| `--fec-parity-blocks` | `usize` | `4` | FEC m parameter (1-64) |
| `--udp-payload-size` | `usize` | `1200` | Fragment payload (500-1472) |

NOTE:

1) The payload size must be under the Ethernet MTU to guarantee functionality, otherwise packets may get fragmented and corrupted. Currently this is clamped to "reasonable" allowed values.

2) All participants of the FTR must specify the same amount of FEC parity and data blocks, as well as share the same trusted relay secret.

3) A lot more configurations can be found and altered in `protocol/trusted_relay/src/params.rs`. This includes options to multi-thread specific workers, etc., but defaults are still under consideration and as such not included in client args.

4) currently ports will always revert to default ports (TCP control -> 16113, UDP transport -> 16114)

## Wire Formats

### Packet (UDP)

The wire format of a singular UDP packet sent over the FTR:

```
[0..32   ] MAC (HMAC-SHA256)
[32..68  ] Header
  [32..64 ] block_hash
  [64..66 ] fragment_index (u16 LE)
  [66..68 ] total_fragments (u16 LE)
[68...   ] Payload (payload_size bytes)
```

### FtrBlock (internal)

This is a pre-encoding of a kaspa block, which includes an explicit block hash, as well as offsets to extract the block header and block txs.

```
[0..32  ] block_hash
[32..36 ] header_len (u32 LE)
[36..40 ] txs_len (u32 LE)
[40...  ] bincode(Header)
[...    ] bincode(Transactions)
```

### TCP Handshake

```
Client → Server:
  [0..32 ] hmac(secret, nonce‖direction‖udp_port)
  [32..64] nonce (random)
  [64    ] direction (0x01=In, 0x02=Out, 0x03=Both)
  [65..67] udp_port (u16 LE)

Server → Client:
  [0     ] 0x01=OK | 0x00=REJECT
```

## Performance Optimizations

1. **Socket Tuning**: 32MB UDP buffers, SO_REUSEPORT for multi-collector
2. **Lock-free Data Plane**: ArcSwap for peer directory snapshots
3. **Bounded Caches**: RingMap/RingSet with LRU eviction
4. **Batch Relay**: Drain-batch pattern amortizes snapshot costs
5. **SIMD FEC**: `reed-solomon-simd` for encoding/decoding
6. **Connected UDP**: Per-peer sockets for better ICMP feedback

Note: The default OS max send & receive buffer sizes may not suffice to properly run the FTR. As such, this value needs to be increased with admin privileges. Currently I am running with 32 MB without issues (although sizes can depend on the number of inbound / outbound FTR peers).

To increase these limits on Linux (in this example to 32 MB) the following can be run:

receive buffer:
`sudo sysctl -w net.core.rmem_max=33554432`
send buffer:
`sudo sysctl -w net.core.wmem_max=33554432`

## Dependencies Added

- `reed-solomon-simd` - SIMD-accelerated FEC codec
- `socket2` - Low-level socket configuration
- `fixedbitset` - Data structure utilized for Compact deduplication tracking

## Testing

- Unit tests are supplied
- Benchmarks in `benches/` for encoder/decoder throughput

## Future Considerations

The main consideration is currently working on stability and proving the concept. Possibly implementing telemetry should be the next phase, for comprehensive data gathering.

Here are some other ideas for the future:

- Explore proper parameter configurations based on real-world observations.
- Implement a Berkeley packet filter for low/kernel-level packet verification (probably important to uplift the concept to an enterprise level).
- Allow for a node to partake in separate FTR clusters.
- Implement alternatives to HMAC verification.
- Implement a lightweight complementary cookie auth.
- Pooling and batching packets via sendmmsg() / recvmmsg() implementations.
- Adaptive FEC based on network conditions.
- Explore compression algorithms.
- Optionally exchange more trust for complete bypassing of consensus verification (facilitating faster time to block template).
- Add a guide to docs
