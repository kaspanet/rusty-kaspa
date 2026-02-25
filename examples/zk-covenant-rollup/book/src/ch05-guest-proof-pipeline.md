# Guest Proof Pipeline

The guest program is the heart of the rollup. It runs inside the RISC Zero zkVM, reads blocks and witness data from the host, processes every transaction, updates the state root, and writes a journal that the on-chain script can verify.

## Overview

```mermaid
flowchart TD
    INPUT["Read PublicInput<br/>(prev_state, prev_seq, covenant_id)"]
    BLOCKS["For each block"]
    TXS["For each transaction"]
    CLASSIFY["Classify: V0 or V1?"]
    V0["V0: read tx_id directly"]
    V1["V1: read payload + rest_preimage<br/>compute tx_id"]
    ACTION["Is action?<br/>(prefix + valid header)"]
    DISPATCH["Dispatch by opcode"]
    TRANSFER["Transfer"]
    ENTRY["Entry"]
    EXIT["Exit"]
    MERKLE["Add seq_commitment_leaf<br/>to block merkle"]
    BLOCK_ROOT["Finalize block merkle root"]
    SEQ["Update seq_commitment"]
    PERM["Build permission tree<br/>(if exits occurred)"]
    JOURNAL["Write journal"]

    INPUT --> BLOCKS
    BLOCKS --> TXS
    TXS --> CLASSIFY
    CLASSIFY -->|V0| V0
    CLASSIFY -->|V1| V1
    V1 --> ACTION
    ACTION -->|Yes| DISPATCH
    ACTION -->|No| MERKLE
    DISPATCH --> TRANSFER --> MERKLE
    DISPATCH --> ENTRY --> MERKLE
    DISPATCH --> EXIT --> MERKLE
    V0 --> MERKLE
    MERKLE --> TXS
    TXS -->|block done| BLOCK_ROOT
    BLOCK_ROOT --> SEQ
    SEQ --> BLOCKS
    BLOCKS -->|all done| PERM
    PERM --> JOURNAL
```

## PublicInput

The guest begins by reading `PublicInput` — three 32-byte hashes that anchor the proof to the chain:

- `prev_state_hash` — the SMT root before this batch
- `prev_seq_commitment` — the sequence commitment before this batch
- `covenant_id` — identifies this specific covenant instance

These values are written to the journal so the on-chain script can verify they match the previous UTXO.

## Block processing

```rust
{{#include ../../methods/guest/src/block.rs:process_block}}
```

Each block contains a list of transactions. The guest processes them sequentially, building a streaming Merkle tree of `seq_commitment_leaf(tx_id, version)` values. The finalized block root is then combined with the running `seq_commitment` via `calc_accepted_id_merkle_root`.

## Transaction classification

```rust
{{#include ../../methods/guest/src/tx.rs:read_v1_tx_data}}
```

For V1 transactions, the guest:

1. Reads the payload bytes from stdin
2. Reads the full `rest_preimage` (length-prefixed) and computes `rest_digest = hash(rest_preimage)` — the guest never trusts a host-provided digest
3. Computes `payload_digest` from the raw payload bytes
4. Computes `tx_id = blake3(payload_digest || rest_digest)`
5. Checks if the `tx_id` starts with `ACTION_TX_ID_PREFIX` (`0x41`)
6. If so, parses the payload as an action header + data

The `rest_preimage` is stored in `V1TxData` and passed to action handlers. For transfer/exit actions, it is used to extract the first input's outpoint (proving which UTXO the transaction actually spends). For entry actions, it is used to parse the deposit output.

The action is only considered valid if the prefix matches **and** the header version and operation are recognized **and** the action-specific validity check passes (e.g., non-zero amount).

## Action parsing

```rust
{{#include ../../methods/guest/src/tx.rs:parse_action}}
```

## Action dispatch

```rust
{{#include ../../methods/guest/src/block.rs:process_action}}
```

## Witness structures

Each action type requires different witness data from the host:

```rust
{{#include ../../methods/guest/src/witness.rs:transfer_witness}}
```

```rust
{{#include ../../methods/guest/src/witness.rs:entry_witness}}
```

```rust
{{#include ../../methods/guest/src/witness.rs:exit_witness}}
```

```mermaid
flowchart LR
    subgraph TransferWit["Transfer Witness"]
        TS["source AccountWitness"]
        TD["dest AccountWitness"]
        TP["PrevTxV1WitnessData<br/>(rest_preimage + payload_digest)"]
    end

    subgraph EntryWit["Entry Witness"]
        ED["dest AccountWitness"]
    end

    subgraph ExitWit["Exit Witness"]
        ES["source AccountWitness"]
        EP["PrevTxV1WitnessData<br/>(rest_preimage + payload_digest)"]
    end
```

**Key simplifications:**

- **Entry witness** no longer includes `rest_preimage`. The current transaction's `rest_preimage` is already read at the `V1TxData` level and passed down.
- **PrevTxV1WitnessData** no longer includes `prev_tx_id` or `output_index`. These are derived from the current action transaction's first input outpoint, which is committed via `rest_preimage` → `rest_digest` → `tx_id`. This prevents the host from substituting a fake previous transaction.

## Source authorization

For transfers and exits, the guest verifies that the action's `source` pubkey matches the public key in a previous transaction output:

```rust
{{#include ../../methods/guest/src/auth.rs:verify_source}}
```

The verification chain is:
1. Guest parses the current action transaction's `rest_preimage` to extract the first input's outpoint `(prev_tx_id, output_index)` — this is committed via `rest_digest` → `tx_id`, so tamper-proof
2. Host provides `PrevTxV1Witness` (rest_preimage + payload_digest of the previous transaction)
3. Guest recomputes the previous `tx_id` from the witness and **asserts** it matches the first input's `prev_tx_id` — mismatch means the host is cheating (proof fails)
4. Guest parses the output at the first input's `output_index` from the previous tx's `rest_preimage`
5. Guest checks the output SPK is Schnorr P2PK format (34 bytes) — if not, the action is **skipped** (user error)
6. Guest extracts the 32-byte pubkey and compares with `action.source` — mismatch is a **skip** (user error)

Only Schnorr P2PK sources are accepted — ECDSA and P2SH sources are rejected.

### Assert vs skip

The guest distinguishes between **host cheating** and **user error**:

| Condition | Response | Rationale |
|-----------|----------|-----------|
| Prev tx witness doesn't hash to first input's tx_id | **Assert** (proof fails) | Host provided fake witness data |
| SMT proof doesn't verify against root | **Assert** (proof fails) | Every pubkey has a valid proof (empty leaf by default) |
| Witness pubkey doesn't match action source | **Assert** (proof fails) | Host should always provide matching witness |
| SPK is not Schnorr P2PK | **Skip** (action rejected) | User submitted action with wrong SPK type |
| SPK pubkey doesn't match action source | **Skip** (action rejected) | User made a mistake in the action payload |
| Insufficient balance | **Skip** (action rejected) | User tried to spend more than they have |

## State updates

```rust
{{#include ../../methods/guest/src/state.rs:process_exit_state}}
```

```rust
{{#include ../../methods/guest/src/state.rs:verify_and_update_dest}}
```

For transfers, the state update is two-phase:
1. **Debit source** — assert SMT proof verifies and witness pubkey matches (host cheating if not), check balance (skip if insufficient), compute intermediate root
2. **Credit destination** — assert SMT proof verifies against intermediate root (host cheating if not), compute final root

For entries, only the credit phase runs (no source debit).

For exits, only the debit phase runs, and a permission leaf is added.

All SMT proof verifications use `assert!` rather than returning `None`, because every pubkey has a valid proof in the sparse Merkle tree (empty leaf by default). If the host provides an invalid proof, it is provably cheating.

## Journal output

```rust
{{#include ../../methods/guest/src/journal.rs:write_output}}
```

The journal is the proof's public output — the only data the on-chain script can see. Its layout:

| Offset | Size | Field |
|--------|------|-------|
| 0 | 32B | `prev_state_hash` |
| 32 | 32B | `prev_seq_commitment` |
| 64 | 32B | `new_state_root` |
| 96 | 32B | `new_seq_commitment` |
| 128 | 32B | `covenant_id` |
| 160 | 32B | `permission_spk_hash` (optional) |

**Base journal:** 160 bytes (40 words) — always present.

**Extended journal:** 192 bytes (48 words) — when exits occurred. The extra 32 bytes contain the blake2b hash of the permission redeem script's P2SH SPK.

## Permission tree construction

When exit actions occur, the guest builds a permission tree:

1. Each successful exit adds `perm_leaf_hash(spk, amount)` to a `StreamingPermTreeBuilder`
2. After all blocks, if any exits occurred:
   - The host provides the expected redeem script length
   - Guest computes the tree root with `pad_to_depth`
   - Guest builds the permission redeem script bytes (using the `no_std` builder in core)
   - Guest asserts the built script length matches the host-provided value
   - Guest computes `blake2b(redeem_script)` → the permission SPK hash
3. This hash is appended to the journal

The on-chain state verification script uses the journal's permission SPK hash to verify that the second covenant output (if present) pays to the correct permission script.
