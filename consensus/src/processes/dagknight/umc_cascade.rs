use std::collections::HashMap;
use std::sync::Arc;

use kaspa_consensus_core::{BlueWorkType, KType};
use kaspa_database::prelude::StoreError;
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use kaspa_math::int::SignedInteger;
use num_traits::Zero;

use crate::model::services::reachability::ReachabilityService;
use crate::processes::dagknight::umc_cascade_persistence::{
    ChainLeafEntry, Mergeset, UmcCascadeKey, UmcCascadePersistedState, UmcCascadeStore,
};
use crate::processes::dagknight::umc_voting::CascadeResult;
use crate::processes::dagknight::{AppendableSegmentTree, Bucket, bucket_for_score};

type SignedWork = SignedInteger<BlueWorkType>;

// ============================================================================
// Cascade Maintainer
// ============================================================================

/// Maintains exact cascade scores for one fixed k using chain decomposition
/// and lazy segment trees with event-driven bucket-transition propagation.
pub struct CascadeMaintainer {
    blues_chains_decomposition: Vec<Vec<Hash>>,
    chains_score_trees: Vec<AppendableSegmentTree<BlockWithWork, SignedWork>>,
    blk_mapping_to_chains: HashMap<Hash, usize>,
    deficit_work: BlueWorkType,
    blue_work: BlueWorkType,
    red_work: BlueWorkType,
    negative_blue_work: BlueWorkType,
    /// Total bucket flips observed during cascade stabilization
    flip_count: u64,
}

/// A block identifier paired with that block's own proof-of-work contribution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockWithWork {
    pub hash: Hash,
    pub work: BlueWorkType,
}

impl BlockWithWork {
    pub fn new(hash: Hash, work: BlueWorkType) -> Self {
        Self { hash, work }
    }
}

fn work_delta(work: BlueWorkType, bucket: Bucket) -> SignedWork {
    let magnitude = SignedWork::from(work);
    match bucket {
        Bucket::Positive => magnitude,
        Bucket::Negative => SignedWork::zero() - magnitude,
    }
}

impl CascadeMaintainer {
    /// Initializes the cascade with `floor(sqrt(k))` conflict-genesis work as its voting deficit.
    pub fn new(conflict_genesis: BlockWithWork, k: KType) -> Self {
        let deficit_work = conflict_genesis.work * u64::from(k.isqrt());
        Self {
            blues_chains_decomposition: Vec::new(),
            chains_score_trees: Vec::new(),
            blk_mapping_to_chains: HashMap::new(),
            deficit_work,
            blue_work: BlueWorkType::ZERO,
            red_work: BlueWorkType::ZERO,
            negative_blue_work: BlueWorkType::ZERO,
            flip_count: 0,
        }
    }

    /// Insert a new blue block into the chain decomposition and stabilize cascade.
    pub fn add_blue(&mut self, block: BlockWithWork, reachability: &impl ReachabilityService) {
        let initial_score = SignedWork::from(self.deficit_work);
        let initial_bucket = bucket_for_score(initial_score);

        let chain_id = self.find_extendable_chain(block.hash, reachability).unwrap_or_else(|| {
            let id = self.blues_chains_decomposition.len();
            self.blues_chains_decomposition.push(Vec::new());
            self.chains_score_trees.push(AppendableSegmentTree::new());
            id
        });

        self.blue_work = self.blue_work + block.work;
        self.append_to_chain(chain_id, block, initial_score, initial_bucket);

        // A new blue block contributes according to its initial bucket.
        let initial_contribution = work_delta(block.work, initial_bucket);
        self.process_event(block.hash, initial_contribution, reachability);
    }

    /// Add a new red block and propagate its negative work to ancestor blues.
    pub fn add_red(&mut self, block: BlockWithWork, reachability: &impl ReachabilityService) {
        self.red_work = self.red_work + block.work;
        self.process_event(block.hash, work_delta(block.work, Bucket::Negative), reachability);
    }

    /// Returns the aggregate score of the virtual block.
    pub fn virtual_score(&self) -> SignedWork {
        SignedWork::from(self.blue_work) + SignedWork::from(self.deficit_work)
            - SignedWork::from(self.red_work)
            - SignedWork::from(self.negative_blue_work * 2)
    }

    /// Check if the virtual block's aggregate cascade score is non-negative.
    pub fn virtual_accepts(&self) -> bool {
        self.virtual_score() >= SignedWork::zero()
    }

    /// Returns the total number of bucket flips observed during cascade stabilization.
    pub fn flip_count(&self) -> u64 {
        self.flip_count
    }

    // ----- Persistence -----

    /// Serialize the current cascade state for checkpoint persistence.
    pub fn to_persisted_state(&mut self, voting_blocks: u64) -> UmcCascadePersistedState {
        let mut chains_leaves: Vec<Vec<ChainLeafEntry>> = Vec::new();

        for (chain_id, _chain) in self.blues_chains_decomposition.iter().enumerate() {
            let tree = &mut self.chains_score_trees[chain_id];
            let leaves = tree.leaves();
            let mut chain_leaves = Vec::new();
            for block_with_work in leaves {
                let score = tree.score(block_with_work);
                let is_negative = score.negative();
                let abs_score: Uint192 = score.abs();
                chain_leaves.push(ChainLeafEntry {
                    hash: block_with_work.hash,
                    work: block_with_work.work,
                    score_abs: abs_score,
                    score_negative: is_negative,
                });
            }
            chains_leaves.push(chain_leaves);
        }

        UmcCascadePersistedState {
            blues_chains_decomposition: self.blues_chains_decomposition.clone(),
            chains_leaves,
            blk_mapping_to_chains: self.blk_mapping_to_chains.clone(),
            deficit_work: self.deficit_work,
            blue_work: self.blue_work,
            red_work: self.red_work,
            negative_blue_work: self.negative_blue_work,
            voting_blocks,
            flip_count: self.flip_count,
        }
    }

    /// Restore cascade state from a persisted checkpoint.
    pub fn from_persisted_state(persisted: &UmcCascadePersistedState, conflict_genesis: BlockWithWork, k: KType) -> Self {
        let mut maintainer = Self::new(conflict_genesis, k);

        // Override counters from persisted state
        maintainer.deficit_work = persisted.deficit_work;
        maintainer.blue_work = persisted.blue_work;
        maintainer.red_work = persisted.red_work;
        maintainer.negative_blue_work = persisted.negative_blue_work;
        maintainer.flip_count = persisted.flip_count;

        // Restore chains and trees
        maintainer.blues_chains_decomposition = persisted.blues_chains_decomposition.clone();
        maintainer.blk_mapping_to_chains = persisted.blk_mapping_to_chains.clone();

        for (chain_id, _chain) in maintainer.blues_chains_decomposition.iter().enumerate() {
            let leaves = &persisted.chains_leaves[chain_id];

            // Rebuild tree from checkpoint
            let mut temp_tree: AppendableSegmentTree<BlockWithWork, SignedWork> = AppendableSegmentTree::new();
            for leaf_entry in leaves {
                let block = BlockWithWork::new(leaf_entry.hash, leaf_entry.work);
                let score: SignedWork = if leaf_entry.score_negative {
                    SignedWork::zero() - SignedWork::from(leaf_entry.score_abs)
                } else {
                    SignedWork::from(leaf_entry.score_abs)
                };
                temp_tree.append_leaf(block, score);
            }

            maintainer.chains_score_trees.push(temp_tree);
        }

        maintainer
    }

    // ----- Chain operations -----

    fn find_extendable_chain(&self, block: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
        for (chain_id, chain) in self.blues_chains_decomposition.iter().enumerate() {
            if let Some(&head) = chain.last()
                && reachability.is_dag_ancestor_of(head, block)
            {
                return Some(chain_id);
            }
        }
        None
    }

    fn append_to_chain(&mut self, chain_id: usize, block: BlockWithWork, initial_score: SignedWork, initial_bucket: Bucket) {
        self.blues_chains_decomposition[chain_id].push(block.hash);
        self.chains_score_trees[chain_id].append_leaf(block, initial_score);
        self.blk_mapping_to_chains.insert(block.hash, chain_id);

        if initial_bucket == Bucket::Negative {
            self.negative_blue_work = self.negative_blue_work + block.work;
        }
    }

    // ----- Event processing -----

    fn process_event(&mut self, source: Hash, delta: SignedWork, reachability: &impl ReachabilityService) {
        let mut queue = Vec::new();

        self.apply_event(source, delta, reachability);

        // Extract crossings and propagate
        let mut changed = true;
        while changed {
            changed = false;

            for tree in self.chains_score_trees.iter_mut() {
                while let Some(block) = tree.extract_positive_below_zero() {
                    tree.flip_to_negative(block);
                    self.negative_blue_work = self.negative_blue_work + block.work;
                    self.flip_count += 1;
                    queue.push((block.hash, work_delta(block.work * 2u64, Bucket::Negative)));
                    changed = true;
                }

                while let Some(block) = tree.extract_negative_at_least_zero() {
                    tree.flip_to_positive(block);
                    self.negative_blue_work = self.negative_blue_work - block.work;
                    self.flip_count += 1;
                    queue.push((block.hash, work_delta(block.work * 2u64, Bucket::Positive)));
                    changed = true;
                }
            }

            for (source, delta) in queue.drain(..) {
                self.apply_event(source, delta, reachability);
            }
        }
    }

    fn apply_event(&mut self, source: Hash, delta: SignedWork, reachability: &impl ReachabilityService) {
        for (chain, tree) in self.blues_chains_decomposition.iter().zip(self.chains_score_trees.iter_mut()) {
            if let Some(ancestor_index) = strict_ancestor_index(chain, source, reachability) {
                tree.prefix_add(ancestor_index, delta);
            }
        }
    }

    /// Serialize checkpoint state at the given chain block for persistence.
    /// and save it to the persistence store
    pub fn save_state(
        &mut self,
        conflict_genesis: Hash,
        k: KType,
        next_chain_ancestor: Hash,
        chain_block: Hash,
        voting_blocks: u64,
        cascade_store: Arc<dyn UmcCascadeStore>,
    ) -> Result<(), StoreError> {
        let persisted_state = self.to_persisted_state(voting_blocks);
        let key = UmcCascadeKey::new(conflict_genesis, k, next_chain_ancestor, chain_block);
        cascade_store.insert_checkpoint(key, persisted_state)
    }
}

/// Returns the exclusive end index of the strict-ancestor prefix, or `None` if it is empty.
fn strict_ancestor_index(chain: &[Hash], source: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = chain.len();

    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let v_mid = chain[mid - 1];
        if reachability.is_dag_ancestor_of(v_mid, source) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    if lo == 0 {
        return None;
    }

    // `lo` is exclusive, so `lo - 1` is the last inclusive ancestor.
    // Reachability includes `source` itself; strict ancestry excludes it.
    if chain[lo - 1] == source {
        lo = lo.saturating_sub(1);
    }

    if lo == 0 {
        return None;
    }

    Some(lo)
}

// ============================================================================
// Checkpoint-Based Cascade Runner
// ============================================================================

/// Run cascade voting on mergesets from the virtual GD chain.
/// Each block's events propagate backward to already-processed strict ancestors.
///
/// `mergeset_stack` is ordered Virtual-first (top-down), so pop() gives CG first.
///
/// Within each mergeset, blocks are already in topological order:
/// - mergeset_blues[i] < mergeset_blues[i+1]
/// - mergeset_reds[i] < mergeset_reds[i+1]
/// - All blues are earlier than all reds
///
/// `next_chain_ancestor` is used to filter grays: a red block that is a chain ancestor
/// of `next_chain_ancestor` is a gray and is skipped.
///
/// `checkpoint_state` is the persisted cascade state loaded by the caller.
/// When `Some`, the cascade reloads from this state and processes only the
/// remaining mergesets above the checkpoint. When `None`, starts from scratch.
///
/// `from_checkpoint` indicates whether we started from a checkpoint (caller's responsibility).
/// `estimated_effort_saved` is the estimated number of blue blocks skipped by checkpointing
/// (caller's responsibility to calculate from virtual_gd.blue_score - checkpoint_blue_score).
/// `estimated_effort_total` is virtual_gd.blue_score (total blues in the conflict zone).
pub fn run_cascade(
    mut mergeset_stack: Vec<Mergeset>,
    conflict_genesis: BlockWithWork,
    k: KType,
    next_chain_ancestor: Hash,
    reachability: &impl ReachabilityService,
    cascade_store: Arc<dyn UmcCascadeStore>,
    checkpoint_state: Option<UmcCascadePersistedState>,
    from_checkpoint: bool,
    estimated_effort_saved: u64,
    estimated_effort_total: u64,
) -> CascadeResult {
    let mut voting_blocks = 0u64;

    // Restore from checkpoint or start fresh
    let mut maintainer = if let Some(persisted) = checkpoint_state {
        voting_blocks = persisted.voting_blocks;
        CascadeMaintainer::from_persisted_state(&persisted, conflict_genesis, k)
    } else {
        CascadeMaintainer::new(conflict_genesis, k)
    };

    // Process remaining mergesets (pop from bottom = CG-first, upward)
    while let Some(mergeset) = mergeset_stack.pop() {
        // Process blues first (already in topological order)
        for (hash, work) in mergeset.mergeset_blues {
            let block_with_work = BlockWithWork::new(hash, work);
            maintainer.add_blue(block_with_work, reachability);
            voting_blocks += 1;
        }

        // Then process reds (already in topological order, skip grays)
        for (hash, work) in mergeset.mergeset_reds {
            let is_gray = reachability.is_chain_ancestor_of(next_chain_ancestor, hash);
            if !is_gray {
                let block_with_work = BlockWithWork::new(hash, work);
                maintainer.add_red(block_with_work, reachability);
                voting_blocks += 1;
            }
        }

        // Checkpoint at chain block — persist to store (best-effort)
        if let Some(chain_block) = mergeset.merging_chain_block {
            let _ = maintainer.save_state(
                conflict_genesis.hash,
                k,
                next_chain_ancestor,
                chain_block,
                voting_blocks,
                cascade_store.clone(),
            );
        }
    }

    let virtual_score = maintainer.virtual_score();
    let accepted = virtual_score >= SignedWork::zero();

    CascadeResult {
        virtual_score,
        accepted,
        flips: maintainer.flip_count,
        voting_blocks,
        from_checkpoint,
        estimated_effort_saved,
        estimated_effort_total,
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use crate::model::services::reachability::MTReachabilityService;
    use crate::model::stores::reachability::{MemoryReachabilityStore, ReachabilityStore};
    use crate::processes::dagknight::umc_cascade_persistence::{
        MemoryUmcCascadeStore, UmcCascadeKey, UmcCascadePersistedState, UmcCascadeStore, UmcCascadeStoreReader,
    };
    use crate::processes::reachability::interval::Interval;
    use kaspa_consensus_core::blockhash::ORIGIN;
    use kaspa_database::prelude::StoreError;
    use rocksdb::WriteBatch;

    /// Test wrapper around MemoryUmcCascadeStore that reports is_persistent()=true
    /// so that run_cascade() actually performs checkpoint saves.
    #[derive(Clone)]
    struct TestPersistentStore {
        inner: MemoryUmcCascadeStore,
    }

    impl TestPersistentStore {
        fn new() -> Self {
            Self { inner: MemoryUmcCascadeStore::new() }
        }
    }

    impl UmcCascadeStoreReader for TestPersistentStore {
        fn get_checkpoint(&self, key: UmcCascadeKey) -> Result<Option<UmcCascadePersistedState>, StoreError> {
            self.inner.get_checkpoint(key)
        }
    }

    impl UmcCascadeStore for TestPersistentStore {
        fn insert_checkpoint(&self, key: UmcCascadeKey, state: UmcCascadePersistedState) -> Result<(), StoreError> {
            self.inner.insert_checkpoint(key, state)
        }

        fn prune_by_conflict_genesis(&self, _batch: &mut WriteBatch, conflict_genesis: Hash) -> Result<u32, StoreError> {
            self.inner.prune_by_conflict_genesis(_batch, conflict_genesis)
        }

        fn is_persistent(&self) -> bool {
            true
        }
    }

    fn make_reachability()
    -> (MTReachabilityService<MemoryReachabilityStore>, std::sync::Arc<parking_lot::RwLock<MemoryReachabilityStore>>) {
        let store = MemoryReachabilityStore::new();
        let arc = std::sync::Arc::new(parking_lot::RwLock::new(store));
        (MTReachabilityService::new(arc.clone()), arc)
    }

    fn reach_insert(arc: &std::sync::Arc<parking_lot::RwLock<MemoryReachabilityStore>>, hash: Hash, parent: Hash, height: u64) {
        let mut store = arc.write();
        store.insert(hash, parent, Interval::new(height, height), height).unwrap();
    }

    fn work() -> BlueWorkType {
        BlueWorkType::from_u64(100)
    }

    #[test]
    fn test_checkpoint_reload_produces_identical_result() {
        // Build a simple DAG: 1→2→3→4→5, conflict genesis at 3
        let (reachability, arc) = make_reachability();
        reach_insert(&arc, Hash::from_u64_word(1), Hash::from_u64_word(0), 1);
        reach_insert(&arc, Hash::from_u64_word(2), Hash::from_u64_word(1), 2);
        reach_insert(&arc, Hash::from_u64_word(3), Hash::from_u64_word(2), 3);
        reach_insert(&arc, Hash::from_u64_word(4), Hash::from_u64_word(3), 4);
        reach_insert(&arc, Hash::from_u64_word(5), Hash::from_u64_word(4), 5);

        let store = Arc::new(TestPersistentStore::new());
        let cg = BlockWithWork::new(Hash::from_u64_word(3), work());
        let k: KType = 0;
        let nca = Hash::from_u64_word(2);

        // Stack: Virtual → Chain4 → CG
        let stack: Vec<Mergeset> = vec![
            Mergeset { merging_chain_block: None, mergeset_blues: vec![(Hash::from_u64_word(5), work())], mergeset_reds: vec![] },
            Mergeset {
                merging_chain_block: Some(Hash::from_u64_word(4)),
                mergeset_blues: vec![(Hash::from_u64_word(4), work())],
                mergeset_reds: vec![],
            },
        ];

        // First run — from scratch
        let result1 = run_cascade(stack.clone(), cg, k, nca, &reachability, store.clone(), None, false, 0, 0);
        assert!(!result1.from_checkpoint);
        assert_eq!(result1.estimated_effort_saved, 0);

        // Second run — with checkpoint state
        // The checkpoint at chain4 should have been saved by run_cascade
        // We simulate the caller loading it and passing it
        let checkpoint_key = UmcCascadeKey::new(cg.hash, k, nca, Hash::from_u64_word(4));
        let checkpoint_state = store.get_checkpoint(checkpoint_key).unwrap();
        assert!(checkpoint_state.is_some(), "checkpoint should have been saved");

        // Retain only mergesets above checkpoint (Virtual only)
        let stack_above_checkpoint: Vec<Mergeset> = stack[..1].to_vec();

        let result2 = run_cascade(
            stack_above_checkpoint,
            cg,
            k,
            nca,
            &reachability,
            store,
            checkpoint_state,
            true,
            1, // estimated_effort_saved estimate
            5, // estimated_effort_total
        );

        assert!(result2.from_checkpoint);
        assert_eq!(result2.estimated_effort_saved, 1);

        // Both should produce identical cascade results
        assert_eq!(result1.accepted, result2.accepted, "accepted mismatch");
        assert_eq!(result1.virtual_score, result2.virtual_score, "virtual score mismatch");
        assert_eq!(result1.flips, result2.flips, "flips mismatch");
    }

    #[test]
    fn test_checkpoint_with_grays_filtered() {
        // Test that gray filtering works correctly with checkpoint reload
        let (reachability, arc) = make_reachability();
        reach_insert(&arc, Hash::from_u64_word(1), Hash::from_u64_word(0), 1);
        reach_insert(&arc, Hash::from_u64_word(2), Hash::from_u64_word(1), 2);
        reach_insert(&arc, Hash::from_u64_word(3), Hash::from_u64_word(2), 3);
        reach_insert(&arc, Hash::from_u64_word(4), Hash::from_u64_word(3), 4);

        let store = Arc::new(TestPersistentStore::new());
        let cg = BlockWithWork::new(Hash::from_u64_word(2), work());
        let k: KType = 0;
        let nca = Hash::from_u64_word(1);

        // Stack with gray block (chain ancestor of NCA)
        let stack: Vec<Mergeset> = vec![
            Mergeset { merging_chain_block: None, mergeset_blues: vec![(Hash::from_u64_word(4), work())], mergeset_reds: vec![] },
            Mergeset {
                merging_chain_block: Some(Hash::from_u64_word(3)),
                mergeset_blues: vec![(Hash::from_u64_word(3), work())],
                mergeset_reds: vec![(Hash::from_u64_word(1), work())], // Gray: chain ancestor of NCA
            },
        ];

        let result1 = run_cascade(stack.clone(), cg, k, nca, &reachability, store.clone(), None, false, 0, 0);

        // Reload from checkpoint
        let checkpoint_key = UmcCascadeKey::new(cg.hash, k, nca, Hash::from_u64_word(3));
        let checkpoint_state = store.get_checkpoint(checkpoint_key).unwrap();
        assert!(checkpoint_state.is_some());

        let result2 = run_cascade(
            stack[..1].to_vec(),
            cg,
            k,
            nca,
            &reachability,
            store.clone(),
            checkpoint_state,
            true,
            1,
            4, // estimated_effort_total
        );

        assert_eq!(result1.accepted, result2.accepted);
        assert_eq!(result1.virtual_score, result2.virtual_score);
        assert!(result2.from_checkpoint);
    }

    #[test]
    fn test_checkpoint_different_nca_different_key() {
        // Test that different NCA produces different checkpoint key
        let (reachability, arc) = make_reachability();
        reach_insert(&arc, Hash::from_u64_word(1), ORIGIN, 1);
        reach_insert(&arc, Hash::from_u64_word(2), Hash::from_u64_word(1), 2);
        reach_insert(&arc, Hash::from_u64_word(3), Hash::from_u64_word(1), 3);

        let store = Arc::new(TestPersistentStore::new());
        let cg = BlockWithWork::new(Hash::from_u64_word(1), work());
        let k: KType = 0;

        // First subgroup: NCA = 2
        let nca_1 = Hash::from_u64_word(2);
        let stack_1: Vec<Mergeset> = vec![
            Mergeset { merging_chain_block: None, mergeset_blues: vec![], mergeset_reds: vec![] },
            Mergeset {
                merging_chain_block: Some(Hash::from_u64_word(2)),
                mergeset_blues: vec![(Hash::from_u64_word(2), work())],
                mergeset_reds: vec![(Hash::from_u64_word(3), work())],
            },
        ];

        let _result1 = run_cascade(stack_1, cg, k, nca_1, &reachability, store.clone(), None, false, 0, 0);

        // Second subgroup: NCA = 3 (different key, should not reuse checkpoint)
        let nca_2 = Hash::from_u64_word(3);
        let stack_2: Vec<Mergeset> = vec![
            Mergeset { merging_chain_block: None, mergeset_blues: vec![], mergeset_reds: vec![] },
            Mergeset {
                merging_chain_block: Some(Hash::from_u64_word(3)),
                mergeset_blues: vec![(Hash::from_u64_word(3), work())],
                mergeset_reds: vec![(Hash::from_u64_word(2), work())],
            },
        ];

        let _result2 = run_cascade(stack_2, cg, k, nca_2, &reachability, store.clone(), None, false, 0, 0);

        // Verify both checkpoints exist with different keys
        let key_1 = UmcCascadeKey::new(cg.hash, k, nca_1, Hash::from_u64_word(2));
        let key_2 = UmcCascadeKey::new(cg.hash, k, nca_2, Hash::from_u64_word(3));

        assert!(store.get_checkpoint(key_1).unwrap().is_some(), "checkpoint for NCA1 should exist");
        assert!(store.get_checkpoint(key_2).unwrap().is_some(), "checkpoint for NCA2 should exist");
    }
}
