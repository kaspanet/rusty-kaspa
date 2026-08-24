//! Generic versioned cache for SMT stores.
//!
//! Provides O(log n) entity-ordered iteration (newest-first) and
//! O(log n) score-based eviction, matching the DB stores' access patterns.
//!
//! # Newest-suffix invariant
//!
//! The cache is designed so that, for every entity, the set of retained
//! versions is a **suffix (by blue_score, newest-first) of that entity's
//! written history**. In other words: if version V for entity E is cached,
//! then every version V' for E with `V'.score > V.score` that was ever
//! written through the incremental `flush` path is also in the cache.
//!
//! This is the invariant [`SmtStores::get_node`] and [`SmtStores::get_lane`]
//! rely on to treat a cache hit as authoritative, without falling back to DB.
//! It rests on the following properties:
//!
//! 1. **Write-through on the incremental path.** Every `flush` that writes a
//!    branch/lane version to DB also inserts the same version into the cache
//!    (see `SmtBuild::flush` and `BlockLaneChanges::flush_lanes`). No version
//!    written incrementally is ever "DB-only".
//! 2. **Score-ordered eviction.** [`insert`](VersionedCache::insert) and
//!    [`evict_below_score`](VersionedCache::evict_below_score) both remove the
//!    lowest-`score` entries first (globally, across all entities). So for a
//!    given entity, the evicted versions are always a prefix of that entity's
//!    history by score, and what remains is a suffix. This preserves the
//!    "newest relevant suffix" needed for authoritative cache hits.
//! 3. **Canonical lookup within a score bucket.** Within a single
//!    `blue_score` bucket, eviction may drop some same-score siblings and
//!    keep others. The eviction tie-break reverses `block_hash` (see
//!    [`ScoreEntityKey`]), so the entry evicted first at a given score is
//!    the one [`VersionedCache::iter_entity`] would yield *last* —
//!    preserving the lowest-block_hash entries, exactly the first
//!    canonical-candidate on a `get` lookup. Correctness doesn't depend on
//!    this (at most one version at a given blue_score is canonical on any
//!    given chain, and DB fallback covers the rest) — the tie-break just
//!    aligns cache retention with the cache's own iteration order for a
//!    better hit rate.
//! 4. **IBD bypass + cold start.** The IBD streaming-import path bypasses the
//!    caches (see `streaming_import` and `db_sink`). This is safe because IBD
//!    runs after [`SmtStores::clear_all`] has emptied both the DB and the
//!    caches. There are no stale cached entries to contradict the imported
//!    state. After IBD, the caches stay cold until incremental writes
//!    repopulate them.

use std::collections::BTreeMap;
use std::fmt::Debug;

use kaspa_hashes::Hash;

/// Trait bound for entity key types used in the versioned cache.
pub trait EntityKey: Ord + Copy + Debug {
    /// Minimum value, used as a lower bound in range queries.
    const MIN: Self;
}

impl EntityKey for Hash {
    const MIN: Self = Hash::from_bytes([0x00; 32]);
}

/// Branch entity key: `(depth, node_key)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchEntity {
    pub depth: u8,
    pub node_key: Hash,
}

impl EntityKey for BranchEntity {
    const MIN: Self = Self { depth: 0, node_key: Hash::from_bytes([0x00; 32]) };
}

/// Key for entity-ordered lookup: iterate versions of an entity newest-first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EntityVersionKey<E: EntityKey> {
    entity: E,
    rev_score: std::cmp::Reverse<u64>,
    block_hash: Hash,
}

/// Key for score-ordered eviction: lowest scores at front. Within a score
/// bucket, the tie-break reverses `block_hash` — so `pop_first` removes the
/// *highest* block_hash at the lowest score. That aligns eviction's "tail"
/// with the entity-scoped iteration tail of [`EntityVersionKey`] (newest
/// score first, then lowest block_hash first), keeping the lowest-block_hash
/// entries — exactly the first canonical-candidate on a `get` — cached for
/// longer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ScoreEntityKey<E: EntityKey> {
    score: u64,
    rev_block_hash: std::cmp::Reverse<Hash>,
    entity: E,
}

/// Versioned cache with entity-ordered iteration and score-based eviction.
///
/// Single-threaded. All mutating operations take `&mut self`.
pub struct VersionedCache<E: EntityKey, V: Copy> {
    by_entity: BTreeMap<EntityVersionKey<E>, V>,
    by_score: BTreeMap<ScoreEntityKey<E>, ()>,
    capacity: usize,
}

impl<E: EntityKey, V: Copy> VersionedCache<E, V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self { by_entity: BTreeMap::new(), by_score: BTreeMap::new(), capacity }
    }

    pub fn len(&self) -> usize {
        self.by_entity.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }

    pub fn clear(&mut self) {
        self.by_entity.clear();
        self.by_score.clear();
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn remaining(&self) -> usize {
        self.capacity.saturating_sub(self.by_entity.len())
    }

    /// Insert a single versioned entry. At capacity, evicts the lowest
    /// entry — lowest blue_score first, then highest block_hash within a
    /// score (see [`ScoreEntityKey`]).
    ///
    /// When the cache is at capacity, the insert is skipped unless the new
    /// entry sorts *strictly above* the current minimum — i.e. it would
    /// only happen via eviction of a strictly lower entry. Admitting a new
    /// entry that would itself be the next eviction candidate would:
    /// 1. Be pure churn — the just-inserted entry pops out on the very
    ///    next eviction, and
    /// 2. Break the newest-suffix invariant for the inserted entity if any
    ///    of its higher versions were already evicted: the cache would
    ///    then hold an entry older than previously-evicted versions of the
    ///    same entity, so a cache hit could shadow a newer canonical
    ///    version that only lives in the DB.
    pub fn insert(&mut self, entity: E, score: u64, block_hash: Hash, value: V) {
        let new_key = ScoreEntityKey { score, rev_block_hash: std::cmp::Reverse(block_hash), entity };
        if self.by_score.len() >= self.capacity {
            let Some(entry) = self.by_score.first_entry() else {
                // Empty-but-at-capacity can only mean capacity 0, which is not currently supported.
                // If it is ever allowed, the cache should admit nothing.
                return;
            };
            if &new_key <= entry.key() {
                // Do not admit an entry that would be the next eviction candidate.
                return;
            }

            let (evicted, ()) = entry.remove_entry();
            self.by_entity.remove(&EntityVersionKey {
                entity: evicted.entity,
                rev_score: std::cmp::Reverse(evicted.score),
                block_hash: evicted.rev_block_hash.0,
            });
        }
        self.by_entity.insert(EntityVersionKey { entity, rev_score: std::cmp::Reverse(score), block_hash }, value);
        self.by_score.insert(new_key, ());
    }

    /// Remove all entries with `score < min_score`. Returns the number of evicted entries.
    ///
    /// Uses `BTreeMap::split_off` to partition at the boundary in O(log n),
    /// then removes the evicted entries from `by_entity`.
    pub fn evict_below_score(&mut self, min_score: u64) -> usize {
        // Split key is the lowest possible `ScoreEntityKey` at `min_score`: the
        // `Reverse<Hash>` minimum is the `Reverse` of the *largest* hash
        // (all 0xFF), and entity's explicit `MIN` matches `EntityKey::MIN`.
        // All entries at `score >= min_score` compare `>= split_key` and are
        // retained; everything at `score < min_score` is evicted.
        let split_key =
            ScoreEntityKey { score: min_score, rev_block_hash: std::cmp::Reverse(Hash::from_bytes([0xFF; 32])), entity: E::MIN };
        let keep = self.by_score.split_off(&split_key);
        let evicted = std::mem::replace(&mut self.by_score, keep);
        let count = evicted.len();
        for entry in evicted.keys() {
            self.by_entity.remove(&EntityVersionKey {
                entity: entry.entity,
                rev_score: std::cmp::Reverse(entry.score),
                block_hash: entry.rev_block_hash.0,
            });
        }
        count
    }

    /// Evict up to `n` entries with the lowest scores. Returns the number actually evicted.
    pub fn evict_oldest_n(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }

        let evicted = if n >= self.by_score.len() {
            std::mem::take(&mut self.by_score)
        } else {
            let split_key = *self.by_score.keys().nth(n).expect("n is below len");
            let keep = self.by_score.split_off(&split_key);
            std::mem::replace(&mut self.by_score, keep)
        };

        let count = evicted.len();
        for entry in evicted.keys() {
            self.by_entity.remove(&EntityVersionKey {
                entity: entry.entity,
                rev_score: std::cmp::Reverse(entry.score),
                block_hash: entry.rev_block_hash.0,
            });
        }
        count
    }

    /// Iterate versions of `entity` from `target_score` downward to `min_score`.
    /// Yields `(score, block_hash, &value)` newest-first.
    pub fn iter_entity(&self, entity: E, target_score: u64, min_score: u64) -> impl Iterator<Item = (u64, Hash, &V)> {
        let start = EntityVersionKey { entity, rev_score: std::cmp::Reverse(target_score), block_hash: Hash::from_bytes([0x00; 32]) };
        self.by_entity
            .range(start..)
            .take_while(move |(k, _)| k.entity == entity && k.rev_score.0 >= min_score)
            .map(|(k, v)| (k.rev_score.0, k.block_hash, v))
    }

    /// Find the first canonical version for `entity` from `target_score` downward.
    pub fn get(
        &self,
        entity: E,
        target_score: u64,
        min_score: u64,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<(u64, Hash, &V)> {
        self.iter_entity(entity, target_score, min_score).find(|(_, bh, _)| is_canonical(*bh))
    }
}

use crate::values::LaneTipHash;
use kaspa_smt::store::Node;

pub type BranchVersionCache = VersionedCache<BranchEntity, Option<Node>>;
pub type LaneVersionCache = VersionedCache<Hash, LaneTipHash>;

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::*;

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    #[test]
    #[ignore]
    fn print_cache_entry_byte_sizes() {
        let lane_logical_entry_bytes = size_of::<Hash>() + size_of::<LaneTipHash>();
        let branch_logical_entry_bytes = size_of::<BranchEntity>() + size_of::<Option<Node>>();
        let lane_indexed_entry_bytes = size_of::<(EntityVersionKey<Hash>, LaneTipHash)>() + size_of::<(ScoreEntityKey<Hash>, ())>();
        let branch_indexed_entry_bytes =
            size_of::<(EntityVersionKey<BranchEntity>, Option<Node>)>() + size_of::<(ScoreEntityKey<BranchEntity>, ())>();

        println!("Hash: {}", size_of::<Hash>());
        println!("LaneTipHash: {}", size_of::<LaneTipHash>());
        println!("BranchEntity: {}", size_of::<BranchEntity>());
        println!("Option<Node>: {}", size_of::<Option<Node>>());
        println!("EntityVersionKey<Hash>: {}", size_of::<EntityVersionKey<Hash>>());
        println!("ScoreEntityKey<Hash>: {}", size_of::<ScoreEntityKey<Hash>>());
        println!("EntityVersionKey<BranchEntity>: {}", size_of::<EntityVersionKey<BranchEntity>>());
        println!("ScoreEntityKey<BranchEntity>: {}", size_of::<ScoreEntityKey<BranchEntity>>());
        println!("lane logical entry bytes: {lane_logical_entry_bytes}");
        println!("branch logical entry bytes: {branch_logical_entry_bytes}");
        println!("lane indexed entry bytes: {lane_indexed_entry_bytes}");
        println!("branch indexed entry bytes: {branch_indexed_entry_bytes}");
        println!("lane budget for 50k entries: {}", lane_indexed_entry_bytes * 50_000);
        println!("branch budget for 500k entries: {}", branch_indexed_entry_bytes * 500_000);
    }

    #[test]
    fn insert_and_get() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);
        let bh = hash(0x11);

        cache.insert(entity, 100, bh, 42);

        let result = cache.get(entity, u64::MAX, 0, |_| true);
        assert_eq!(result, Some((100, bh, &42)));
        assert_eq!(cache.remaining(), 99);
    }

    #[test]
    fn iter_entity_newest_first() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);

        cache.insert(entity, 100, hash(0x11), 1);
        cache.insert(entity, 200, hash(0x22), 2);
        cache.insert(entity, 150, hash(0x33), 3);

        let results: Vec<_> = cache.iter_entity(entity, u64::MAX, 0).collect();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].0, 200);
        assert_eq!(results[1].0, 150);
        assert_eq!(results[2].0, 100);
    }

    #[test]
    fn iter_entity_respects_target_and_min_score() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);

        cache.insert(entity, 100, hash(0x11), 1);
        cache.insert(entity, 200, hash(0x22), 2);
        cache.insert(entity, 300, hash(0x33), 3);

        let results: Vec<_> = cache.iter_entity(entity, 250, 0).collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 200);
        assert_eq!(results[1].0, 100);

        let results: Vec<_> = cache.iter_entity(entity, u64::MAX, 150).collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 300);
        assert_eq!(results[1].0, 200);
    }

    #[test]
    fn get_with_canonicality_filter() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);
        let canonical_bh = hash(0x22);

        cache.insert(entity, 200, hash(0x11), 1);
        cache.insert(entity, 150, canonical_bh, 2);
        cache.insert(entity, 100, hash(0x33), 3);

        let result = cache.get(entity, u64::MAX, 0, |bh| bh == canonical_bh);
        assert_eq!(result, Some((150, canonical_bh, &2)));
    }

    #[test]
    fn evict_below_score_returns_count() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);

        cache.insert(entity, 100, hash(0x11), 1);
        cache.insert(entity, 200, hash(0x22), 2);
        cache.insert(entity, 300, hash(0x33), 3);

        let evicted = cache.evict_below_score(200);
        assert_eq!(evicted, 1);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn evict_oldest_n() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);

        cache.insert(entity, 100, hash(0x11), 1);
        cache.insert(entity, 200, hash(0x22), 2);
        cache.insert(entity, 300, hash(0x33), 3);
        cache.insert(entity, 400, hash(0x44), 4);

        assert_eq!(cache.evict_oldest_n(0), 0);
        assert_eq!(cache.len(), 4);

        assert_eq!(cache.evict_oldest_n(1), 1);
        assert_eq!(cache.len(), 3);
        let results: Vec<_> = cache.iter_entity(entity, u64::MAX, 0).collect();
        assert_eq!(results.iter().map(|(score, _, _)| *score).collect::<Vec<_>>(), vec![400, 300, 200]);

        assert_eq!(cache.evict_oldest_n(2), 2);
        assert_eq!(cache.len(), 1);
        let results: Vec<_> = cache.iter_entity(entity, u64::MAX, 0).collect();
        assert_eq!(results[0].0, 400);

        cache.insert(entity, 500, hash(0x55), 5);
        assert_eq!(cache.evict_oldest_n(usize::MAX), 2);
        assert!(cache.is_empty());
    }

    #[test]
    fn capacity_eviction() {
        let mut cache = VersionedCache::<Hash, u64>::new(2);
        let entity = hash(1);

        cache.insert(entity, 100, hash(0x11), 1);
        cache.insert(entity, 200, hash(0x22), 2);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.remaining(), 0);

        cache.insert(entity, 300, hash(0x33), 3);
        assert_eq!(cache.len(), 2);

        let results: Vec<_> = cache.iter_entity(entity, u64::MAX, 0).collect();
        assert_eq!(results[0].0, 300);
        assert_eq!(results[1].0, 200);
    }

    #[test]
    fn different_entities_independent() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let e1 = hash(1);
        let e2 = hash(2);

        cache.insert(e1, 100, hash(0x11), 1);
        cache.insert(e2, 200, hash(0x22), 2);

        let r1: Vec<_> = cache.iter_entity(e1, u64::MAX, 0).collect();
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0].2, &1);

        let r2: Vec<_> = cache.iter_entity(e2, u64::MAX, 0).collect();
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0].2, &2);
    }

    #[test]
    fn branch_entity_key() {
        let mut cache = VersionedCache::<BranchEntity, u64>::new(100);
        let entity = BranchEntity { depth: 0, node_key: hash(0) };
        let bh = hash(0x11);

        cache.insert(entity, 100, bh, 42);
        let result = cache.get(entity, u64::MAX, 0, |_| true);
        assert_eq!(result, Some((100, bh, &42)));
    }

    /// Newest-suffix invariant under cross-entity capacity pressure.
    ///
    /// Eviction is score-ordered globally, so when versions from several
    /// entities compete for a small capacity, some entity's older versions
    /// are dropped while other entities keep theirs. For each entity, the
    /// retained cached versions must still form a blue-score-newest suffix
    /// of that entity's write history — otherwise a cache hit could skip a
    /// newer canonical version that only lives in the DB, breaking the
    /// "cache hit is authoritative" guarantee relied on by
    /// `SmtStores::get_node` / `get_lane`.
    #[test]
    fn newest_suffix_invariant_under_cross_entity_pressure() {
        let capacity = 5;
        let mut cache = VersionedCache::<Hash, u64>::new(capacity);
        let entities = [hash(0xA), hash(0xB), hash(0xC)];

        // Per-entity histories of inserted scores (ascending — blocks are
        // written in blue-score order per entity).
        let mut histories: std::collections::BTreeMap<Hash, Vec<u64>> = std::collections::BTreeMap::new();

        // Interleaved, score-ascending-per-entity writes. Total writes > capacity
        // so eviction fires repeatedly with varying lowest-score entity.
        let writes: &[(usize, u64)] =
            &[(0, 100), (1, 50), (0, 200), (2, 75), (1, 150), (0, 300), (2, 175), (1, 250), (2, 225), (0, 400), (1, 350)];

        for &(ei, score) in writes {
            let entity = entities[ei];
            // block_hash derived from score so cache entries are identifiable.
            let bh = Hash::from_bytes([score as u8; 32]);
            cache.insert(entity, score, bh, score);
            histories.entry(entity).or_default().push(score);
        }

        assert_eq!(cache.len(), capacity, "cache must be at capacity after > capacity writes");

        // For each entity, the retained scores (newest-first) must equal the
        // suffix of its write history with the same length. Equivalently: no
        // "gap" where a lower-score version is cached but a strictly-higher
        // version for the same entity was written yet evicted.
        for entity in &entities {
            let retained_newest_first: Vec<u64> = cache.iter_entity(*entity, u64::MAX, 0).map(|(score, _, _)| score).collect();

            let Some(history) = histories.get(entity) else {
                continue;
            };
            let k = retained_newest_first.len();
            assert!(k <= history.len());

            // History is ascending; the k-element suffix newest-first is:
            let expected_suffix_newest_first: Vec<u64> = history.iter().rev().take(k).copied().collect();
            assert_eq!(
                retained_newest_first, expected_suffix_newest_first,
                "entity {entity:?}: cached versions {retained_newest_first:?} are not the newest suffix of history {history:?}",
            );
        }

        // Stronger formulation of the same invariant: for every entity, every
        // written version strictly newer than the oldest cached version for
        // that entity must itself be cached. (A counterexample would mean a
        // newer canonical version is missing from the cache while an older
        // one is retained — exactly the scenario a "cache hit is
        // authoritative" lookup must never observe.)
        for entity in &entities {
            let retained_scores: std::collections::BTreeSet<u64> =
                cache.iter_entity(*entity, u64::MAX, 0).map(|(s, _, _)| s).collect();
            let Some(oldest_cached) = retained_scores.iter().next().copied() else {
                continue;
            };
            let history = &histories[entity];
            for &s in history.iter().filter(|&&s| s >= oldest_cached) {
                assert!(
                    retained_scores.contains(&s),
                    "entity {entity:?}: score {s} in history is newer than cached {oldest_cached} but missing from cache",
                );
            }
        }
    }

    /// Under a small cache and interleaved multi-entity writes, `get()` under
    /// a canonicality predicate must never return a stale (older canonical)
    /// version when a newer canonical version was written — even if that
    /// newer version is also cached alongside non-canonical siblings at the
    /// same score. Exercises the fast-path authoritative-read guarantee.
    #[test]
    fn fast_path_does_not_return_stale_under_same_score_siblings() {
        let mut cache = VersionedCache::<Hash, u64>::new(4);
        let entity = hash(1);

        // Non-canonical and canonical siblings at score 100, plus an older
        // canonical 80 and a newer non-canonical 120.
        let non_canon_100 = hash(0xAA);
        let canon_100 = hash(0xCC);
        let canon_80 = hash(0xBB);
        let non_canon_120 = hash(0xDD);

        cache.insert(entity, 80, canon_80, 80);
        cache.insert(entity, 100, non_canon_100, 100);
        cache.insert(entity, 100, canon_100, 101);
        cache.insert(entity, 120, non_canon_120, 120);

        let is_canonical = |bh: Hash| bh == canon_100 || bh == canon_80;

        // Read at target >= 120: first canonical cached hit should be canon_100
        // (non_canon_120 is filtered, canon_100 is the newest canonical).
        let hit = cache.get(entity, u64::MAX, 0, is_canonical).unwrap();
        assert_eq!(hit.1, canon_100);
        assert_eq!(hit.0, 100);
        assert_eq!(hit.2, &101);

        // Read at target between 80 and 100: only canon_80 is in range for
        // the canonicality predicate.
        let hit = cache.get(entity, 95, 0, is_canonical).unwrap();
        assert_eq!(hit.1, canon_80);
        assert_eq!(hit.0, 80);
    }

    /// After three same-score inserts into a capacity-2 cache, the two
    /// lowest-block_hash entries always survive — regardless of insertion
    /// order. The highest-block_hash entry either gets evicted on arrival of
    /// a lower-hash sibling, or is skipped on insert (because a new entry
    /// that would itself be the next eviction candidate is not admitted —
    /// see [`VersionedCache::insert`]).
    ///
    /// This pins down both the `Reverse<Hash>` tie-break in
    /// [`ScoreEntityKey`] and the "only replace lowest with strictly higher"
    /// rule in `insert`: together they ensure the first canonical-candidate
    /// position at any score (the lowest block_hash, yielded first by
    /// `iter_entity`) is preserved over higher-hash siblings.
    #[test]
    fn same_score_eviction_retains_lowest_block_hashes() {
        let low_hash = hash(0x11);
        let mid_hash = hash(0x55);
        let high_hash = hash(0xFF);

        let orderings = [
            [low_hash, mid_hash, high_hash],
            [low_hash, high_hash, mid_hash],
            [mid_hash, low_hash, high_hash],
            [mid_hash, high_hash, low_hash],
            [high_hash, low_hash, mid_hash],
            [high_hash, mid_hash, low_hash],
        ];

        let expected: std::collections::BTreeSet<Hash> = [low_hash, mid_hash].into_iter().collect();

        for order in orderings {
            let mut cache = VersionedCache::<Hash, u64>::new(2);
            let entity = hash(1);
            for (i, bh) in order.iter().enumerate() {
                cache.insert(entity, 100, *bh, i as u64);
            }
            assert_eq!(cache.len(), 2);

            let retained: std::collections::BTreeSet<Hash> = cache.iter_entity(entity, u64::MAX, 0).map(|(_, bh, _)| bh).collect();

            assert_eq!(
                retained, expected,
                "same-score eviction must retain the two lowest block_hashes regardless of insert order (order {order:?}, retained {retained:?})",
            );
        }
    }

    /// Cross-entity capacity pressure at the same score: when a newly-inserted
    /// same-score entry forces eviction, the evicted entry must be the
    /// highest-block_hash existing entry across all entities at that score,
    /// not the lowest. This verifies the `Reverse<Hash>` ordering extends
    /// across entities (the tie-break in [`ScoreEntityKey`] is `rev_block_hash`
    /// *before* `entity`).
    #[test]
    fn same_score_eviction_picks_highest_hash_across_entities() {
        let mut cache = VersionedCache::<Hash, u64>::new(2);
        let x = hash(1);
        let y = hash(2);
        let low = hash(0x10);
        let high = hash(0xF0);

        cache.insert(x, 100, low, 10);
        cache.insert(y, 100, high, 20);
        // Cache is full; inserting a new same-score entry evicts one existing.
        // With `Reverse<Hash>` tie-break, the highest-hash existing entry
        // (y, 0xF0) is evicted regardless of entity, so x's low-hash entry
        // survives.
        let z = hash(3);
        cache.insert(z, 100, hash(0x80), 30);

        let x_entries: Vec<Hash> = cache.iter_entity(x, u64::MAX, 0).map(|(_, bh, _)| bh).collect();
        let y_entries: Vec<Hash> = cache.iter_entity(y, u64::MAX, 0).map(|(_, bh, _)| bh).collect();
        let z_entries: Vec<Hash> = cache.iter_entity(z, u64::MAX, 0).map(|(_, bh, _)| bh).collect();

        assert_eq!(x_entries, vec![low], "x's lower-hash entry must be retained");
        assert!(y_entries.is_empty(), "y's higher-hash entry must be evicted");
        assert_eq!(z_entries, vec![hash(0x80)], "newly-inserted entry must survive");
    }

    /// `evict_below_score` must evict *all* entries at scores strictly below
    /// the cutoff, regardless of block_hash, and must retain every entry at
    /// `score >= cutoff`
    #[test]
    fn evict_below_score_preserves_all_entries_at_cutoff() {
        let mut cache = VersionedCache::<Hash, u64>::new(100);
        let entity = hash(1);

        // Populate score 99 (all should be evicted) and score 100 (all
        // should be retained) with both extremes of the block_hash range.
        for bh_byte in [0x00u8, 0x7F, 0xFF] {
            cache.insert(entity, 99, Hash::from_bytes([bh_byte; 32]), bh_byte as u64);
            cache.insert(entity, 100, Hash::from_bytes([bh_byte; 32]), bh_byte as u64);
        }
        assert_eq!(cache.len(), 6);

        let evicted = cache.evict_below_score(100);
        assert_eq!(evicted, 3, "all three entries at score 99 must be evicted");
        assert_eq!(cache.len(), 3, "all three entries at score 100 must be retained");

        // Every retained entry should be at exactly score 100, across all
        // block_hash values (both 0x00 and 0xFF extremes must survive).
        let mut retained: Vec<(u64, Hash)> = cache.iter_entity(entity, u64::MAX, 0).map(|(s, bh, _)| (s, bh)).collect();
        retained.sort_unstable_by_key(|&(_, bh)| bh);
        assert_eq!(
            retained,
            vec![(100, Hash::from_bytes([0x00; 32])), (100, Hash::from_bytes([0x7F; 32])), (100, Hash::from_bytes([0xFF; 32])),],
        );
    }

    /// `insert` at a blue_score strictly below the cache's current minimum
    /// must not push out a higher-score version when the cache is at
    /// capacity. If it did, an entity whose newer versions have already been
    /// evicted would end up with a cached version older than its
    /// previously-evicted ones — breaking the newest-suffix invariant that
    /// `get_node` / `get_lane` rely on to treat cache hits as authoritative.
    ///
    /// Concrete scenario: a block processed out of blue_score order (e.g. a
    /// fork block with low blue_score processed after the tip was at high
    /// blue_score) writes an entity update at a score below every entry
    /// currently in cache. The newly inserted low-score version would itself
    /// be the very next eviction candidate, so inserting it is pure loss —
    /// and evicting a legitimately-cached higher-score entry to make room
    /// silently corrupts the cache's per-entity suffix for any entity whose
    /// older versions had already been purged.
    #[test]
    fn insert_below_current_min_must_not_violate_newest_suffix() {
        // Capacity 2 keeps the eviction path easy to trigger deterministically.
        let mut cache = VersionedCache::<Hash, u64>::new(2);
        let e_x = hash(1);
        let e_y = hash(2);
        let e_z = hash(3);

        // Step 1 — write E_x twice (150, then 170). Cache fills with E_x's
        // two versions.
        cache.insert(e_x, 150, hash(0x15), 150);
        cache.insert(e_x, 170, hash(0x17), 170);

        // Step 2 — capacity pressure from other entities evicts both of
        // E_x's cached versions in turn. After this, the cache has no E_x
        // entries, and E_x's write history is {150, 170} (both persisted to
        // DB via write-through, but no longer in cache).
        cache.insert(e_y, 200, hash(0x20), 200);
        cache.insert(e_z, 300, hash(0x30), 300);
        assert!(cache.iter_entity(e_x, u64::MAX, 0).next().is_none(), "precondition: E_x has been fully evicted from cache");

        // Step 3 — out-of-order post-restart / fork write at blue_score 50
        // for E_x. Because 50 < every score currently in cache (200, 300),
        // this entry would itself be the next eviction candidate. Inserting
        // it evicts a legitimately-cached higher-score entry for some other
        // entity *and* leaves E_x's cache holding an entry older than its
        // previously-evicted versions.
        cache.insert(e_x, 50, hash(0x05), 50);

        // Invariant: for every entity in cache, the retained versions
        // newest-first must be a prefix of the entity's write history
        // sorted newest-first. Equivalently: no cached entry for an entity
        // may be older than an already-written (and possibly-evicted)
        // version of the same entity.
        let histories: [(Hash, &[u64]); 3] = [(e_x, &[150, 170, 50]), (e_y, &[200]), (e_z, &[300])];
        for (entity, history) in histories {
            let retained: Vec<u64> = cache.iter_entity(entity, u64::MAX, 0).map(|(s, _, _)| s).collect();
            let mut history_desc: Vec<u64> = history.to_vec();
            history_desc.sort_unstable_by(|a, b| b.cmp(a));
            let expected = &history_desc[..retained.len()];
            assert_eq!(
                retained, expected,
                "entity {entity:?}: cached scores {retained:?} are not a newest-first suffix of write history {history:?}",
            );
        }
    }
}
