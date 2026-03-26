//! Generic versioned cache for SMT stores.
//!
//! Provides O(log n) entity-ordered iteration (newest-first) and
//! O(log n) score-based eviction, matching the DB stores' access patterns.

use std::collections::{BTreeMap, BTreeSet};
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

/// Branch entity key: `(height, node_key)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BranchEntity {
    pub height: u8,
    pub node_key: Hash,
}

impl EntityKey for BranchEntity {
    const MIN: Self = Self { height: 0, node_key: Hash::from_bytes([0x00; 32]) };
}

/// Key for entity-ordered lookup: iterate versions of an entity newest-first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EntityVersionKey<E: EntityKey> {
    entity: E,
    rev_score: std::cmp::Reverse<u64>,
    block_hash: Hash,
}

/// Key for score-ordered eviction: lowest scores at front.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ScoreEntityKey<E: EntityKey> {
    score: u64,
    block_hash: Hash,
    entity: E,
}

/// Versioned cache with entity-ordered iteration and score-based eviction.
///
/// Single-threaded. All mutating operations take `&mut self`.
pub struct VersionedCache<E: EntityKey, V: Copy> {
    by_entity: BTreeMap<EntityVersionKey<E>, V>,
    by_score: BTreeSet<ScoreEntityKey<E>>,
    capacity: usize,
}

impl<E: EntityKey, V: Copy> VersionedCache<E, V> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self { by_entity: BTreeMap::new(), by_score: BTreeSet::new(), capacity }
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

    /// Insert a single versioned entry. Evicts the lowest-score entry if at capacity.
    pub fn insert(&mut self, entity: E, score: u64, block_hash: Hash, value: V) {
        if self.by_score.len() >= self.capacity
            && let Some(evicted) = self.by_score.pop_first()
        {
            self.by_entity.remove(&EntityVersionKey {
                entity: evicted.entity,
                rev_score: std::cmp::Reverse(evicted.score),
                block_hash: evicted.block_hash,
            });
        }
        self.by_entity.insert(EntityVersionKey { entity, rev_score: std::cmp::Reverse(score), block_hash }, value);
        self.by_score.insert(ScoreEntityKey { score, block_hash, entity });
    }

    /// Remove all entries with `score < min_score`. Returns the number of evicted entries.
    ///
    /// Uses `BTreeSet::split_off` to partition at the boundary in O(log n),
    /// then removes the evicted entries from `by_entity`.
    pub fn evict_below_score(&mut self, min_score: u64) -> usize {
        let split_key = ScoreEntityKey { score: min_score, block_hash: Hash::from_bytes([0x00; 32]), entity: E::MIN };
        let keep = self.by_score.split_off(&split_key);
        let evicted = std::mem::replace(&mut self.by_score, keep);
        let count = evicted.len();
        for entry in &evicted {
            self.by_entity.remove(&EntityVersionKey {
                entity: entry.entity,
                rev_score: std::cmp::Reverse(entry.score),
                block_hash: entry.block_hash,
            });
        }
        count
    }

    /// Evict up to `n` entries with the lowest scores. Returns the number actually evicted.
    pub fn evict_oldest_n(&mut self, n: usize) -> usize {
        let mut evicted = 0;
        for _ in 0..n {
            if let Some(first) = self.by_score.pop_first() {
                self.by_entity.remove(&EntityVersionKey {
                    entity: first.entity,
                    rev_score: std::cmp::Reverse(first.score),
                    block_hash: first.block_hash,
                });
                evicted += 1;
            } else {
                break;
            }
        }
        evicted
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

use crate::values::LaneVersion;
use kaspa_smt::store::BranchChildren;

pub type BranchVersionCache = VersionedCache<BranchEntity, BranchChildren>;
pub type LaneVersionCache = VersionedCache<Hash, LaneVersion>;

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
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

        let evicted = cache.evict_oldest_n(2);
        assert_eq!(evicted, 2);
        assert_eq!(cache.len(), 1);
        let results: Vec<_> = cache.iter_entity(entity, u64::MAX, 0).collect();
        assert_eq!(results[0].0, 300);
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
        let entity = BranchEntity { height: 255, node_key: hash(0) };
        let bh = hash(0x11);

        cache.insert(entity, 100, bh, 42);
        let result = cache.get(entity, u64::MAX, 0, |_| true);
        assert_eq!(result, Some((100, bh, &42)));
    }
}
