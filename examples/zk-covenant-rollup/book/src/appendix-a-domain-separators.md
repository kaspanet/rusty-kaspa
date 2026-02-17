# Appendix A: Domain Separators

Every hash in the system uses domain separation to prevent cross-protocol collisions. This appendix lists all domain tags, their hash function, and purpose.

## Hash domain tags

| Domain string | Hash | Keyed? | Module | Purpose |
|---------------|------|--------|--------|---------|
| `"SMTLeaf"` | SHA-256 | No (prefix) | `core/src/smt.rs` | Account leaf: `sha256("SMTLeaf" \|\| pubkey \|\| balance)` |
| `"SMTEmpty"` | SHA-256 | No (prefix) | `core/src/smt.rs` | Empty account slot sentinel: `sha256("SMTEmpty")` |
| `"SMTBranch"` | SHA-256 | No (prefix) | `core/src/smt.rs` | SMT internal node: `sha256("SMTBranch" \|\| left \|\| right)` |
| `"PermLeaf"` | SHA-256 | No (prefix) | `core/src/permission_tree.rs` | Withdrawal leaf: `sha256("PermLeaf" \|\| spk \|\| amount)` |
| `"PermEmpty"` | SHA-256 | No (prefix) | `core/src/permission_tree.rs` | Empty withdrawal slot: `sha256("PermEmpty")` |
| `"PermBranch"` | SHA-256 | No (prefix) | `core/src/permission_tree.rs` | Permission tree node: `sha256("PermBranch" \|\| left \|\| right)` |
| `"SeqCommitmentMerkleLeafHash"` | BLAKE3 | Yes (key) | `core/src/seq_commit.rs` | Seq commitment leaf: `blake3_keyed(key, tx_id \|\| version)` |
| `"SeqCommitmentMerkleBranchHash"` | BLAKE3 | Yes (key) | `core/src/seq_commit.rs` | Seq commitment node: `blake3_keyed(key, left \|\| right)` |
| `"PayloadDigest"` | BLAKE3 | Yes (key) | `core/src/lib.rs` | V1 tx payload hash |
| `"TransactionRest"` | BLAKE3 | Yes (key) | `core/src/lib.rs` | V1 tx rest-of-data hash |
| `"TransactionV1Id"` | BLAKE3 | Yes (key) | `core/src/lib.rs` | V1 tx_id: `blake3_keyed(key, payload_digest \|\| rest_digest)` |
| `"TransactionID"` | BLAKE2b-256 | Yes (.key()) | `core/src/lib.rs` | V0 tx_id: `blake2b_keyed(key, full_preimage)` |

## Non-hash domain tags

| Tag | Value | Type | Module | Purpose |
|-----|-------|------|--------|---------|
| `ACTION_TX_ID_PREFIX` | `0x41` (`'A'`) | Byte prefix | `core/src/lib.rs` | First byte of tx_id identifies action transactions |
| State verification suffix | `[0x00, 0x75]` | Opcode pair | `host/src/bridge.rs` | `[OP_0, OP_DROP]` tags state verification scripts |
| Permission suffix | `[0x51, 0x75]` | Opcode pair | `host/src/bridge.rs` | `[OP_1, OP_DROP]` tags permission scripts |

## Hashing strategy

The system uses three hash functions, chosen to match Kaspa's protocol:

**SHA-256** — Used for Merkle trees that must be replicated on-chain via `OP_SHA256`. Both the account SMT and permission tree use SHA-256 with domain-prefix separation (`sha256(tag || data)`).

**BLAKE3** — Used for transaction IDs and sequence commitments. Kaspa's V1 transaction ID scheme uses BLAKE3 with keyed hashing. The `domain_to_key()` function zero-pads a domain string into a 32-byte BLAKE3 key:

```rust
{{#include ../../core/src/lib.rs:226-234}}
```

**BLAKE2b-256** — Used for V0 transaction IDs (legacy) and P2SH script hashing. V0 tx_id uses keyed BLAKE2b; P2SH uses unkeyed BLAKE2b matching `kaspa_txscript::pay_to_script_hash_script`.

## Why separate tree domains?

The SMT and permission tree intentionally use different domain strings (`"SMTLeaf"` vs `"PermLeaf"`, etc.) even though both use SHA-256. This prevents a valid proof in one tree from being accepted in the other. A test in `permission_tree.rs` explicitly asserts:

```rust
assert_ne!(perm_empty_leaf_hash(), crate::smt::empty_leaf_hash());
```
