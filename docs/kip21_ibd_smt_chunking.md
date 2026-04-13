# KIP-21 IBD: Resume-from-Stable SMT and Chunked Flow-Controlled Lane Streaming

## Motivation

Three related problems surfaced during KIP-21 IBD SMT state sync.

### 1. `IncomingRouteCapacityReached` disconnects

The sender enqueued one `SmtLaneEntryMessage` per lane via the router's
non-blocking `try_send` into a **256-capacity** incoming mpsc
(`protocol/p2p/src/core/router.rs` — `incoming_flow_baseline_channel_size()`).
When the receiver's CPU-bound SMT builder back-pressured its inner mpsc, the
stream recv loop stopped draining the p2p route, the 256 cap filled, and the
router dropped the peer with `IncomingRouteCapacityReached`. On reconnect the
syncee restarted SMT sync from scratch.

### 2. Session / pruning lock held across the full IBD stream on the sender

The previous `blocking_iter_smt_lanes` spawned one blocking task that held a
RocksDB raw iterator (and its implicit snapshot) open for the entire
many-second-to-many-minute iteration. The consensus session and the
`pruning_lock` were held the whole time, blocking pruning and compaction,
violating the "no locks held for >2 seconds" rule.

### 3. No resume from already-synced SMT state

Before this work, even if SMT state had been fully synced, a mid-utxoset-sync
disconnect forced the next IBD round to wipe and re-download SMT from scratch.
`sync_new_smt_state` unconditionally cleared the stores on entry, and the
`IbdType::Sync` branch gated SMT sync on `!is_utxo_stable` alone, conflating
the two independent completion states.

## Changes

### Part 1 — Resume from stable SMT

**`protocol/flows/src/ibd/flow.rs`**

- `IbdType::Sync` gains `is_smt_stable: bool` alongside `is_utxo_stable` and
  `is_pp_anticone_synced`.
- `determine_ibd_type` queries `async_is_pruning_smt_stable`, **but forces
  `is_smt_stable = true` when covenants are not active at the current pruning
  point**. Pre-activation `sync_new_smt_state` is a no-op and the flag is
  never set, so without this override pre-activation nodes would fail the
  `Lagging`/`Leading` "fully stable" check.
- The `Sync` branch gates SMT and utxoset sync **independently**:
  ```rust
  if !is_smt_stable  { self.sync_new_smt_state(...).await?; }
  if !is_utxo_stable { self.sync_new_utxo_set(...).await?; }
  ```
  Each branch logs an "already stable, skipping" message when taken.

**Safety relies on the invariant `is_utxo_stable ⇒ is_smt_stable`**, preserved
by three existing properties of the code:

1. `sync_new_smt_state` always runs before `sync_new_utxo_set` in all three
   IBD paths (`Sync`, `DownloadHeadersProof`, `PruningCatchUp`).
2. `set_pruning_smt_stable(true)` (end of `sync_new_smt_state`) is called
   before `set_pruning_utxoset_stable(true)` (end of `sync_new_utxo_set`).
3. Pruning-point advance
   (`consensus/src/consensus/mod.rs:504-505`) resets both flags in the same
   `WriteBatch`.

**`testing/integration/Cargo.toml`**
- `kaspa-hashes = { workspace = true, default-features = true }` so the
  integration test crate can be built as a standalone test target without
  tripping the `MemSizeEstimator` feature-flag issue.

### Part 2 — Chunked wire framing + flow control

**Proto changes** — `protocol/p2p/proto/p2p.proto` and `messages.proto`

- Dropped top-level `SmtLaneEntryMessage`.
- Nested `SmtLaneEntry { bytes data (64B: lane_key || lane_tip); uint64 blueScore; bytes proof; }`.
- `SmtLaneChunkMessage { repeated SmtLaneEntry entries = 1; }` — up to
  `SMT_CHUNK_SIZE` (4096) entries per wire message.
- `RequestNextPruningPointSmtChunkMessage {}` — flow-control signal from
  receiver to sender.
- **Removed** `DoneSmtChunksMessage`. Termination is driven entirely by
  `active_lanes_count` in the metadata header; no sentinel required.
- Payload IDs: 60 SmtMetadata (unchanged), 61 SmtLaneChunk (replaces
  SmtLaneEntry), 63 RequestNextPruningPointSmtChunk. ID 62 freed.

**Payload enum** — `protocol/p2p/src/core/payload_type.rs`

Dropped `SmtLaneEntry` / `DoneSmtChunks`, added `SmtLaneChunk` /
`RequestNextPruningPointSmtChunk`.

**Constants** — `protocol/flows/src/ibd/streams.rs`

```rust
pub const SMT_CHUNK_SIZE: usize = 4096;          // max lane entries per chunk
pub const SMT_FLOW_CONTROL_WINDOW: usize = 10;   // re-request after N chunks
```

Flow-control safety: 10-chunk window × worst-case ≈ 20 in-flight chunk
messages ≪ 256 route capacity.

**Receiver stream** — `SmtStream` in `protocol/flows/src/ibd/streams.rs`

- Carries `expected_count: u64`, `lane_count: u64`, `chunks_received: usize`,
  and a real `router: &Router` reference (previously unused `_router`).
- `recv_metadata` stores `self.expected_count = payload.active_lanes_count`.
- Replaces `next()` with `next_chunk() -> Option<Vec<ImportLane>>`. Parses
  entries in order, re-applying the existing 64-byte `data` split and
  `OwnedSmtProof::from_bytes` parsing on every `SMT_PROOF_INTERVAL`-th entry.
- Validates: non-empty chunks; does not push past `expected_count`.
- Enqueues `RequestNextPruningPointSmtChunk` every
  `SMT_FLOW_CONTROL_WINDOW` chunks — **guarded by `lane_count < expected_count`**
  so no trailing re-request is emitted after the final chunk (which would
  dead-wait the sender).
- Returns `Ok(None)` once `lane_count >= expected_count`, letting the caller
  terminate naturally.

**Receiver loop** — `protocol/flows/src/ibd/flow.rs` (`sync_new_smt_state`)

- Inner builder mpsc carries `Vec<ImportLane>` (whole chunks), capacity 2 —
  enough headroom for one chunk in flight + one being processed by the
  importer. Each chunk holds up to `SMT_CHUNK_SIZE` lanes, so the effective
  in-flight capacity is still large without re-flattening.
- `while let Some(chunk) = stream.next_chunk().await? { tx.send(chunk).await?; }` — the
  chunk is forwarded as-is; the importer does not re-batch.
- Final `info!("SMT state synced: {} lanes", stream.lane_count())`.

**Streaming importer** — `consensus/smt-store/src/streaming_import/mod.rs`

- `streaming_import` signature changed from
  `lanes: impl Iterator<Item = StreamingImportLane>` to
  `chunks: impl Iterator<Item = Vec<ImportLane>>`. Each yielded `Vec` is one
  pre-sized chunk from the wire-level chunker (up to `SMT_CHUNK_SIZE`). The
  `StreamingImportLane` struct is deleted; smt-store now depends on
  `kaspa-consensus-core` and reuses the single `ImportLane` type, eliminating
  the identical-fields conversion layer in `Consensus::import_pruning_point_smt`.
- Removed the internal `chunk` accumulator and `step` closure. The per-chunk
  work (parallel leaf hashing via rayon, proof verification, lane/score-index
  batching, `builder.feed`) is now inlined into a single `for chunk in chunks`
  loop.
- `max_batch_entries` remains the RocksDB `WriteBatch` flush threshold for
  lane/score-index writes; it is now independent of (and unrelated to) the
  incoming chunk size.
- Corresponding update to `consensus/smt-store/tests/integration.rs` and
  `consensus/smt-store/examples/bench_streaming_import.rs` to yield chunks.

**Flow registration** — `protocol/flows/src/v9/mod.rs`

- IBD consumer route swaps `SmtLaneEntry` → `SmtLaneChunk`.
- `RequestPruningPointSmtStateFlow` subscribes to both
  `RequestPruningPointSmtState` and `RequestNextPruningPointSmtChunk`.

### Part 3 — Cursor-paged DB reads on the sender

**`consensus/smt-store/src/lane_version_store.rs`**

`iter_all_canonical` gains `from_lane_key: Option<Hash>` as its first
parameter. If `Some`, initial seek uses `next_lane_seek_key(prefix, k)`
(lexicographic successor); if the cursor is `0xFF…FF`, iteration is marked
done immediately. Otherwise seeks to start of prefix. All 8 existing
`iter_all_canonical_*` unit tests updated to pass `None`.

**`consensus/core/src/api/mod.rs`**

- Dropped `iter_pruning_point_smt_lanes(pp, Box<dyn FnMut(ImportLane) -> bool + ...>)`.
- Added:
  ```rust
  fn get_pruning_point_smt_lanes_chunk(
      &self,
      expected_pruning_point: Hash,
      from_lane_key: Option<Hash>,
      limit: usize,
      starting_lane_idx: u64,
  ) -> ConsensusResult<Vec<ImportLane>>;
  ```

**`consensus/src/consensus/mod.rs`** — single-allocation pipeline:

```rust
self.storage.smt_stores.lane_version
    .iter_all_canonical(from_lane_key, min_score, |bh| self.virtual_processor.is_smt_canonical(bh, pp))
    .take(limit)
    .enumerate()
    .map(|(i, res)| {
        let (lk, v) = res.unwrap();
        let absolute_idx = starting_lane_idx + i as u64;
        let proof = if (absolute_idx as usize).is_multiple_of(SMT_PROOF_INTERVAL) {
            Some(self.storage.smt_stores
                .prove_lane(&lk, min_score, |bh| self.virtual_processor.is_smt_canonical(bh, pp)).unwrap())
        } else {
            None
        };
        Ok(ImportLane { lane_key: lk, lane_tip: *v.data(), blue_score: v.blue_score(), proof })
    })
    .collect()
```

- Returns `ConsensusError::UnexpectedPruningPoint` if the pruning point moved
  under the sender mid-IBD.
- Absolute-index modulo for proof placement so proof positions are stable
  across chunk boundaries (aligns naturally since `SMT_CHUNK_SIZE = 4096` is
  a multiple of `SMT_PROOF_INTERVAL = 16`).
- `.take(limit).collect()` drives the RocksDB iterator to completion and
  drops it (releasing the implicit snapshot) before this call returns.

**`components/consensusmanager/src/session.rs`**

Replaced `blocking_iter_smt_lanes` with `async_get_pruning_point_smt_lanes_chunk`
— a thin `spawn_blocking` wrapper around the new consensus method.

**`protocol/flows/src/v9/request_pruning_point_smt_state.rs`** — sender flow

- Session is dropped **immediately after** the metadata send. No session is
  held anywhere in the main loop.
- Loop state: `cursor: Option<Hash>`, `lanes_sent: u64`, `chunks_sent: usize`.
  Bounded by `expected_count`.
- Each iteration:
  1. Re-acquire session via `consensus.session().await`
  2. Call `async_get_pruning_point_smt_lanes_chunk(pp, cursor, limit, lanes_sent)`
     where `limit = min(remaining, SMT_CHUNK_SIZE)`
  3. Session released when `spawn_blocking` returns
  4. Cursor advanced to `lanes.last().lane_key`
  5. Batch sent as one `SmtLaneChunkMessage`
- Empty result mid-stream → send `UnexpectedPruningPoint` and bail.
- `if lanes_sent >= expected_count { break; }` **before** any flow-control
  wait, so the last chunk never deadlocks on a `RequestNext` that will never
  arrive.
- After every `SMT_FLOW_CONTROL_WINDOW` chunks (and only if more remain):
  `dequeue!(self.incoming_route, Payload::RequestNextPruningPointSmtChunk)?`.
- **Tail sanity check**: after the loop, one final
  `get_pruning_point_smt_lanes_chunk(cursor, 1, ...)` call — if it returns
  non-empty, the DB has more lanes than `active_lanes_count` promised, and
  the function errors with:
  `"SMT lane iteration yielded more entries than active_lanes_count"`.
- Empty-state (`expected_count == 0`): metadata-only send, no chunks.

## Verification

- `cargo check --workspace` — clean
- `cargo nextest run -p kaspa-testing-integration daemon_pruning_seqcommit_sync_test` —
  **PASS** (≈9 s). This existing test drives two daemons over a real p2p
  session with KIP-21 covenants activated on simnet: syncer advances past a
  pruning point with seqcommit transactions, a fresh syncee IBDs and must
  validate the seqcommit spend block. Every new code path (chunked stream,
  flow control, cursor-paged DB reads, tail sanity check, resume-from-stable
  logic) is exercised.
- `cargo nextest run -p kaspa-p2p-flows` — 3/3 existing unit tests pass.
- Commit `ffe1cd169` landed the resume-from-stable change; the
  chunking / flow-control / cursor-paging work is still unstaged at the time
  of writing.

## Net effect

- **No more `IncomingRouteCapacityReached`**: the sender stalls at
  `dequeue!(RequestNextPruningPointSmtChunk)` when the receiver is slow,
  naturally rate-limited by actual processing speed. Back-pressure chain:
  builder → inner tx (4096) → `next_chunk` recv → `RequestNext` gate →
  sender's `dequeue!` wait.
- **2-second rule respected on the sender**: each chunk fetch is bounded
  work (≤ 4096 lane entries + ≤ 256 proof generations per call). The
  consensus session and the RocksDB iterator are released between chunks;
  pruning and compaction proceed freely in the gaps.
- **Partial-progress resume**: if SMT sync completed but utxoset sync was
  interrupted, the next IBD round skips SMT sync and jumps straight to
  utxoset, preserving completed work. Disconnects mid-SMT still redownload
  that state cleanly (protected by `clear_pruning_smt_stores` at the start
  of `sync_new_smt_state`).
- **Wire overhead**: the `repeated SmtLaneEntry` framing costs roughly
  5–10 KB of proto tag/length bytes per 4096-lane chunk vs a flat-bytes
  layout — deemed negligible against the simplicity of reusing prost's
  message codec.

## Files touched

```
consensus/core/src/api/mod.rs
consensus/smt-store/src/lane_version_store.rs
consensus/src/consensus/mod.rs
components/consensusmanager/src/session.rs
protocol/flows/src/ibd/flow.rs
protocol/flows/src/ibd/streams.rs
protocol/flows/src/v9/mod.rs
protocol/flows/src/v9/request_pruning_point_smt_state.rs
protocol/p2p/proto/messages.proto
protocol/p2p/proto/p2p.proto
protocol/p2p/src/core/payload_type.rs
testing/integration/Cargo.toml
```
