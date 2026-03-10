# KIP-21 Active Lanes SMT — Versioned Store (Implementation Design)

> Single RocksDB default CF, prefix-separated keys (Kaspa convention).
> All multi-byte integers in keys use **ReverseBlueScore** (`u64::MAX - blue_score`, big-endian)
> so forward lexicographic iteration yields latest versions first.
> Crate: `consensus/smt-store/` (`kaspa-smt-store`).

---

## Changes from Previous Design (design-2)

1. **Removed PrevPtr / linked lists.** Version values no longer embed `(prev_blue_score, prev_hash)` pointers. The score index + RocksDB key ordering provide all traversal needed — no per-entity linked list required.

2. **Removed tombstones.** No separate "branch became empty" or "lane purged" marker values. Old versions stay in the DB and are filtered by blue score threshold. Branch/lane head deletion handles the "became empty" case.

3. **Reversed score ordering.** Keys use `ReverseBlueScore` (`u64::MAX - blue_score`) instead of raw `blue_score`. Forward RocksDB iteration yields highest blue scores first, eliminating the need for `seek_for_prev`. A simple `seek` + forward scan finds the latest version.

4. **Score index redesign.** Key is `prefix | rev_blue_score | block_hash` (41 bytes). Value is concatenated lane keys (`N * 32` bytes) rather than one entry per lane. One key per block instead of one key per (block, lane) pair.

5. **Canonicality via reachability, not linked lists.** `get` methods accept an `is_canonical(block_hash) -> bool` predicate. The iterator walks versions from highest blue score downward; the first entry passing the canonicality check is returned. No linked-list walk needed.

6. **`min_blue_score` bound on all iterators.** Both `get` and `get_at` accept a `min_blue_score` parameter. The iterator stops early when it hits a version below this threshold, enabling efficient pruning-aware reads.

7. **`MaybeFork<T>` / `Verified<T>` wrappers.** `get_at` returns `MaybeFork<T>` (caller must verify canonicality). `get` returns `Verified<T>` (canonicality already checked). Both carry `blue_score` and `block_hash` alongside the data and expose `into_parts() -> (T, u64, Hash)`.

8. **Explicit `blue_score` naming.** All parameters, fields, and methods use `blue_score` (not generic `score`) to distinguish from DAA score.

9. **Actual prefix values.** Uses `0x46`–`0x4A` (decimal 70–74) registered in `DatabaseStorePrefixes`, not the pseudocode `0x01`–`0x05`.

---

## Concept

Every branch node and lane record is **versioned by `(blue_score, block_hash)`**.
No data is ever deleted on rollback or reorg — only on pruning.

Version stores are **append-only**: `apply_block` writes new entries, rollback only
moves head pointers backward. Historical reads use the score-ordered key layout
with a canonicality predicate to skip fork entries.

```
  get(entity, min_blue_score=900, is_canonical)
      │
      ▼  seek to rev_blue_score(u64::MAX) for this entity
      │  iterate forward (= descending blue_score)
      │
      ├─ blue_score=1000, hash=D  ← is_canonical(D)? YES → return Verified
      ├─ blue_score=1000, hash=E  ← (would check if D failed)
      ├─ blue_score=950,  hash=C  ← ...
      ├─ blue_score=900,  hash=B  ← last candidate (min_blue_score=900)
      └─ blue_score=850,  hash=A  ← STOP (below min_blue_score)
```

---

## Primitives

```
Hash         — 32 bytes (kaspa_hashes::Hash, zerocopy-enabled)
ReverseBlueScore — u64::MAX - blue_score, big-endian 8 bytes
               Forward iteration = descending blue_score order.

BranchKey {
    height:   u8,     // 0 = leaf level, 255 = root child
    node_key: Hash,   // lane_key with bottom (height+1) bits zeroed
}

leaf_hash(lane) = H_leaf(lane_id_bytes || lane_tip_hash || be_u64(blue_score))
```

---

## Stores

All keys include a 1-byte prefix as the first field. Prefixes are registered in
`database/src/registry.rs` as `DatabaseStorePrefixes`.

### Store 1 — Branch Head Pointers (`0x46` / 70)

One entry per branch that has ever been written. Points to the current canonical
version by `block_hash`.

```
Key:   0x46 | height(1) | node_key(32)     = 34 bytes
Value: block_hash(32)

Rust: BranchHeadKey { prefix, height, node_key }
```

- **Absent** = branch never written = empty subtree.
- **apply:** set to `B.hash` for every branch B changes.
- **rollback:** restore previous head (obtained from version store iteration).
- **pruning:** delete when the branch has no version in the retention window.

---

### Store 2 — Branch Versions (`0x47` / 71)

One immutable entry per `(branch, block)` pair where the branch value changed.
Written once by `apply_block`, never modified.

```
Key:   0x47 | height(1) | node_key(32) | rev_blue_score(8) | block_hash(32)  = 74 bytes
Value: left(32) | right(32)                                                   = 64 bytes

Rust: BranchVersionKey { prefix, height, node_key, rev_blue_score, block_hash }
      BranchVersion { left, right }

Entity prefix (for iteration bounds): prefix(1) | height(1) | node_key(32) = 34 bytes
```

**Why `blue_score` in the key:**
- **Range-delete pruning:** `delete_range` prunes all versions below a cutoff in one call.
- **O(log N) historical entry point:** `seek(entity | rev_blue_score(target) | 0x00..00)` lands at the newest version at or before `target` in one LSM seek.
- **Exact key construction:** rollback and flip-reorg construct the exact key `entity | rev_blue_score(B.blue_score) | B.hash`.

**Methods:**
- `put(writer, height, node_key, blue_score, block_hash, &BranchVersion)`
- `delete(writer, height, node_key, blue_score, block_hash)`
- `get(height, node_key, min_blue_score, is_canonical) -> Option<Verified<BranchVersion>>`
- `get_at(height, node_key, target_blue_score, min_blue_score) -> impl Iterator<Item = MaybeFork<BranchVersion>>`

---

### Store 3 — Lane Head Pointers (`0x48` / 72)

One entry per lane that has ever been active.

```
Key:   0x48 | lane_key(32)   = 33 bytes
Value: block_hash(32)

Rust: LaneHeadKey { prefix, lane_key }
```

Same advance / restore semantics as Store 1.

---

### Store 4 — Lane Versions (`0x49` / 73)

One immutable entry per `(lane, block)` pair where the lane was touched.

```
Key:   0x49 | lane_key(32) | rev_blue_score(8) | block_hash(32)  = 73 bytes
Value: lane_id(20) | lane_tip_hash(32)                            = 52 bytes

Rust: LaneVersionKey { prefix, lane_key, rev_blue_score, block_hash }
      LaneVersion { lane_id, lane_tip_hash }

Entity prefix: prefix(1) | lane_key(32) = 33 bytes
```

`blue_score` from the key doubles as `last_touch_score` — extracted directly,
not duplicated in the value.

`leaf_hash` is omitted from the value — fully deterministic as
`H_leaf(lane_id || tip_hash || be_u64(blue_score_from_key))`, computed on demand.

**Methods:** same pattern as Store 2 but keyed by `lane_key` instead of `(height, node_key)`.

---

### Store 5 — Score Index (`0x4A` / 74)

Append-only record of all lane touches per block. One entry per block.
Entries are never deleted on rollback — pruned by score range only.

```
Key:   0x4A | rev_blue_score(8) | block_hash(32)   = 41 bytes
Value: lane_key_0(32) | lane_key_1(32) | …          = N * 32 bytes

Rust: ScoreIndexKey { prefix, rev_blue_score, block_hash }
      Value: &[Hash] (zerocopy slice)
```

**Uses:**
1. **Rollback lane discovery:** iterate from `B.blue_score` to find all lanes B touched.
   Filter by `block_hash == B.hash` to exclude non-canonical blocks at the same score.
2. **Inactivity eviction:** forward-scan from low scores to find stale lanes.
3. **Pruning traversal:** iterate score range to collect version keys for deletion
   from Stores 2 and 4. Branch keys are deterministic from lane keys
   (`node_key = lane_key with bottom (height+1) bits zeroed`).

**Methods:**
- `put(writer, blue_score, block_hash, &[Hash])`
- `get_at(target_blue_score, min_blue_score) -> impl Iterator<Item = MaybeFork<Vec<Hash>>>`
- `delete_range(writer, up_to_blue_score)` — removes all entries with `blue_score <= up_to_blue_score`

---

## Zerocopy Serialization

All keys and values derive `zerocopy::{FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned}`.
Keys implement `AsRef<[u8]>` via `as_bytes()` for direct use with RocksDB `get_pinned`/`put`/`delete`.

Deserialization uses:
- `T::ref_from_bytes(slice)` for keys (zero-copy reference into RocksDB buffer)
- `T::read_from_bytes(slice)` for values (copy into owned struct)
- `<[Hash]>::ref_from_bytes(slice)` for score index values (zero-copy hash slice)

---

## Reading — `get` and `get_at`

### `get(entity, min_blue_score, is_canonical)`

Finds the latest canonical version above `min_blue_score`:

```
fn get(entity, min_blue_score, is_canonical) -> Option<Verified<V>>:
    for entry in get_at(entity, u64::MAX, min_blue_score):
        if is_canonical(entry.block_hash()):
            return Some(entry.into_verified())
    return None
```

### `get_at(entity, target_blue_score, min_blue_score)`

Returns an iterator of `MaybeFork<V>` from `target_blue_score` downward to `min_blue_score`:

```
fn get_at(entity, target_blue_score, min_blue_score) -> Iterator<MaybeFork<V>>:
    seek(entity_prefix | rev_blue_score(target_blue_score) | 0x00..00)

    loop:
        if !iter.valid():
            check iter.status() for I/O errors
            break

        key = parse_key(iter.key())
        if key not in entity_prefix: break
        if key.blue_score() < min_blue_score: break

        value = parse_value(iter.value())
        yield MaybeFork::new(value, key.blue_score(), key.block_hash)
        iter.next()
```

**Cost:** O(log N) seek + O(fork versions skipped) forward steps. In the common case
(no fork near target), the first entry is canonical and returns immediately.

---

## Operation Flows

### apply_block(B)

```
fn apply_block(B):
    batch = WriteBatch::new()

    for lane_key in touched_lanes(B):
        new = compute_lane(old, B)

        // Lane version (Store 4)
        batch.put(Store4, lane_key, B.blue_score, B.hash, new)

        // Lane head (Store 3)
        batch.put(Store3, lane_key, B.hash)

        // Walk SMT path (Stores 1 + 2)
        leaf = H_leaf(new.lane_id || new.tip_hash || be_u64(B.blue_score))
        walk_up(batch, lane_key, leaf, B)

    // Score index (Store 5) — one entry for the whole block
    batch.put(Store5, B.blue_score, B.hash, touched_lane_keys)

    db.write(batch)
```

### walk_up(batch, lane_key, leaf_hash, B)

```
fn walk_up(batch, lane_key, leaf_hash, B):
    current = leaf_hash

    for height in 0..=255:
        bk         = BranchKey::new(height, lane_key)
        old_branch = Store2.get(height, bk.node_key, 0, is_canonical)

        goes_right    = bit_at(lane_key, 255 - height)
        sibling       = old_branch.map(|b| if goes_right { b.left } else { b.right })
                            .unwrap_or(EMPTY_HASHES[height])
        (left, right) = if goes_right { (sibling, current) } else { (current, sibling) }
        parent        = hash_node(left, right)

        if Some((left, right)) == old_branch.map(|b| (b.left, b.right)):
            current = parent
            continue   // branch unchanged — all ancestors also unchanged

        // Write branch version (Store 2)
        batch.put(Store2, height, bk.node_key, B.blue_score, B.hash,
                  BranchVersion { left, right })

        // Advance branch head (Store 1)
        batch.put(Store1, height, bk.node_key, B.hash)

        current = parent
```

### rollback_block(B)

Undoes the effect of block B on head pointers. No version data is deleted.

```
fn rollback_block(B):
    batch = WriteBatch::new()

    // 1. Discover lanes B touched via score index (Store 5)
    touched_lanes = Store5.get_at(B.blue_score, B.blue_score)
                        .filter(|e| e.block_hash() == B.hash)
                        .flat_map(|e| e.into_parts().0)

    // 2. Step lane heads back (Store 3 + Store 4)
    for lane_key in touched_lanes:
        // Find the previous canonical version before B
        prev = Store4.get_at(lane_key, B.blue_score - 1, 0)
                     .find(|e| is_canonical(e.block_hash()))
        if prev is Some:
            batch.put(Store3, lane_key, prev.block_hash())
        else:
            batch.delete(Store3, lane_key)

    // 3. Step branch heads back (Store 1 + Store 2)
    for lane_key in touched_lanes:
        for height in 0..=255:
            bk   = BranchKey::new(height, lane_key)
            head = Store1.get(height, bk.node_key)
            if head != B.hash: continue

            prev = Store2.get_at(height, bk.node_key, B.blue_score - 1, 0)
                         .find(|e| is_canonical(e.block_hash()))
            if prev is Some:
                batch.put(Store1, height, bk.node_key, prev.block_hash())
            else:
                batch.delete(Store1, height, bk.node_key)

    db.write(batch)
```

### prune(cutoff_blue_score)

```
fn prune(cutoff_blue_score):
    // 1. Iterate score index from cutoff downward to collect lane/branch keys
    for entry in Store5.get_at(cutoff_blue_score, 0):
        for lane_key in entry.data():
            // Delete lane versions at or below cutoff
            for v in Store4.get_at(lane_key, cutoff_blue_score, 0):
                batch.delete(Store4, lane_key, v.blue_score(), v.block_hash())
            // Delete branch versions — node_keys are deterministic from lane_key
            for height in 0..=255:
                node_key = lane_key with bottom (height+1) bits zeroed
                for v in Store2.get_at(height, node_key, cutoff_blue_score, 0):
                    batch.delete(Store2, height, node_key, v.blue_score(), v.block_hash())

    // 2. Range-delete score index entries
    Store5.delete_range(writer, cutoff_blue_score)

    db.write(batch)
```

---

## Store Summary

| # | Name | Prefix | Key | Value | Append-only? |
|---|---|---|---|---|---|
| 1 | Branch heads | `0x46` (70) | `height(1) \| node_key(32)` = 34B | `block_hash(32)` | No |
| 2 | Branch versions | `0x47` (71) | `height(1) \| node_key(32) \| rev_blue_score(8) \| block_hash(32)` = 74B | `left(32) \| right(32)` = 64B | **Yes** |
| 3 | Lane heads | `0x48` (72) | `lane_key(32)` = 33B | `block_hash(32)` | No |
| 4 | Lane versions | `0x49` (73) | `lane_key(32) \| rev_blue_score(8) \| block_hash(32)` = 73B | `lane_id(20) \| lane_tip_hash(32)` = 52B | **Yes** |
| 5 | Score index | `0x4A` (74) | `rev_blue_score(8) \| block_hash(32)` = 41B | `lane_keys(N×32)` | **Yes** |

**Stores 2, 4, and 5 are append-only until pruning.**
Nothing is deleted on rollback or reorg.

---

## Invariants

1. **Head–version consistency:** for every `Store1[height, node_key] = hash`, a version
   entry exists in Store 2 at the corresponding `(height, node_key, blue_score, hash)` key.
   Same for Store 3 / Store 4.

2. **Score index completeness:** every lane touch in block B produces exactly one
   Store 5 entry at `(B.blue_score, B.hash)` with all touched lane keys in the value.

3. **No redundant versions:** a branch version is written only when the branch value
   actually changes (`walk_up` skips unchanged branches).

4. **Canonicality by predicate:** fork versions exist in Stores 2 and 4 but are
   filtered out by `is_canonical` checks using the reachability service.

---

## Properties

| Property | Detail |
|---|---|
| Current read | `get(entity, 0, is_canonical)` — seek to `u64::MAX`, first canonical hit. |
| Historical read | `get(entity, min_blue_score, is_canonical)` — same seek, stops at bound. |
| Write per branch per block | 1 version (74B key + 64B value) + 1 head update (34B key + 32B value). |
| Write per lane per block | 1 version (73B key + 52B value) + 1 head update (33B key + 32B value). |
| Write per block (score index) | 1 entry (41B key + N×32B value). |
| Rollback | Score index lookup → lane list → iterate version stores for previous canonical heads. Zero writes to version stores. |
| Pruning | Iterate score index → collect keys → batch delete from Stores 2, 4. Range-delete Store 5. |
| RAM requirement | No in-memory canonical set. Reachability service provides O(1) ancestor checks. |
