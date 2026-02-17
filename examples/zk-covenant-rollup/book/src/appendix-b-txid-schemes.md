# Appendix B: Transaction ID Schemes

Kaspa supports two transaction ID formats. The rollup guest must handle both to verify previous transaction outputs.

## V0: BLAKE2b full preimage

V0 transactions compute their ID by hashing the entire serialized transaction:

```rust
{{#include ../../core/src/lib.rs:tx_id_v0}}
```

The domain key `"TransactionID"` matches Kaspa's `kaspa_hashes::TransactionID` hasher exactly. The full transaction bytes (version, inputs, outputs, locktime, subnetwork, gas, payload) are hashed in one pass.

**Verification in the guest:**

```rust
{{#include ../../core/src/prev_tx.rs:prev_tx_v0_compute}}
```

The host provides the full transaction preimage. The guest hashes it, compares against the claimed tx_id, then parses the preimage to extract the output SPK at the claimed index.

## V1: BLAKE3 split digest

V1 transactions split the hash into two components:

```
tx_id = blake3_keyed("TransactionV1Id", payload_digest || rest_digest)
```

where:
- `payload_digest = blake3_keyed("PayloadDigest", payload_bytes)`
- `rest_digest = blake3_keyed("TransactionRest", rest_preimage)`

```rust
{{#include ../../core/src/lib.rs:payload_digest}}
```

```rust
{{#include ../../core/src/lib.rs:tx_id_v1}}
```

**Verification in the guest:**

```rust
{{#include ../../core/src/prev_tx.rs:prev_tx_v1_compute}}
```

The host provides:
1. The pre-computed `payload_digest` (32 bytes) — the guest does not need the full payload
2. The `rest_preimage` — the guest hashes this to get `rest_digest` and parses it for output data

## Why the split matters

The V1 split design benefits the ZK guest:

1. **Smaller witness:** The guest only needs `payload_digest` (32 bytes) instead of the full payload (variable, potentially large). The payload contains the action data which the guest already has from block processing.

2. **Output extraction:** The `rest_preimage` contains the outputs, so the guest can extract the output SPK for deposit verification without needing the payload bytes.

3. **Efficient hashing:** BLAKE3 is faster than BLAKE2b in the RISC-V guest, and the split allows partial preimage reuse.

## Witness structures

The `PrevTxWitness` enum handles both versions:

| Version | Witness type | Host provides | Guest computes |
|---------|-------------|---------------|----------------|
| V0 | `PrevTxV0Witness` | Full preimage | `blake2b(preimage)` |
| V1 | `PrevTxV1Witness` | `rest_preimage` + `payload_digest` | `blake3(payload_digest \|\| blake3(rest))` |

## Kaspa compatibility

Both implementations are tested against Kaspa's canonical hashers:

- `tx_id_v0` matches `kaspa_hashes::TransactionID`
- `payload_digest` matches `kaspa_hashes::PayloadDigest`
- `rest_digest` matches `kaspa_hashes::TransactionRest`
- `tx_id_v1` matches `kaspa_hashes::TransactionV1Id`

See `core/src/lib.rs` tests for the compatibility assertions.
