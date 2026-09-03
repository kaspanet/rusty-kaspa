use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    sync::{Arc, OnceLock},
};

use dashmap::DashMap;
use parking_lot::RwLock;

use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, BlueWorkType, HashKTypeMap, HashMapCustomHasher, KType,
    blockhash::{self, BlockHashExtensions, BlockHashes},
};
use kaspa_database::prelude::{StoreError, StoreResultUnitExt};
use kaspa_hashes::Hash;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            dagknight::{DagknightKey, DagknightStore, DagknightStoreReader},
            ghostdag::GhostdagData,
            headers::HeaderStoreReader,
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::{
        dagknight::umc_voting::ColoringReader,
        difficulty::calc_work,
        ghostdag::{
            mergeset::unordered_mergeset_without_selected_parent,
            ordering::SortableBlock,
            protocol::{ChainBlock, ColoringOutput, ColoringState},
        },
        reachability::relations::FutureIntersectRelations,
    },
};

/// Granular lock key for k-colouring operations.
/// Uniquely identifies a subgroup's processing context by its conflict genesis,
/// k-value, next chain ancestor (NCA), and search type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KColouringLockKey {
    conflict_genesis: Hash,
    k: KType,
    nca: Hash,
    free_search: bool,
}

impl KColouringLockKey {
    /// Creates a lock key for committed search.
    /// The NCA must be a block whose selected parent is the conflict genesis.
    pub fn committed_search_key(conflict_genesis: Hash, k: KType, nca: Hash) -> Self {
        Self { conflict_genesis, k, nca, free_search: false }
    }

    /// Creates a lock key for free search.
    /// In free search, the NCA defaults to the conflict genesis itself.
    pub fn free_search_key(conflict_genesis: Hash, k: KType) -> Self {
        Self { conflict_genesis, k, nca: conflict_genesis, free_search: true }
    }

    pub fn conflict_genesis(&self) -> Hash {
        self.conflict_genesis
    }

    pub fn is_free_search(&self) -> bool {
        self.free_search
    }
}

// Global lock map for granular k-colouring synchronization.
static K_COLOURING_LOCKS: OnceLock<DashMap<KColouringLockKey, Arc<RwLock<()>>>> = OnceLock::new();

fn get_k_colouring_locks() -> &'static DashMap<KColouringLockKey, Arc<RwLock<()>>> {
    K_COLOURING_LOCKS.get_or_init(DashMap::new)
}

/// Cleans up unused locks from the global k-colouring lock map.
/// A lock is considered unused if its Arc strong count is 1 (only the map holds a reference).
fn cleanup_k_colouring_locks() {
    if let Some(locks) = K_COLOURING_LOCKS.get() {
        // TODO[DK]: Track average lock map size as part of metrics for later
        locks.retain(|_, v| Arc::strong_count(v) > 1);
    }
}

// START Copied from GD Manager
// NOTE: This is a copy from GD Manager right now, but the idea here is that it will update k_colouring to
// be more in line with what the paper needs
// Renamed from ghostdag_customized to k_colouring
pub struct ConflictZoneManager<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader,
    D: RelationsStoreReader,
    R: ReachabilityStoreReader + Clone,
> {
    k: KType,
    root: Hash,
    free_search: bool,
    dagknight_store: Arc<C>,
    headers_store: Arc<O>,
    relations_store: FutureIntersectRelations<D, MTReachabilityService<R>>,
    reachability_service: MTReachabilityService<R>,
}

impl<C: DagknightStore + DagknightStoreReader, O: HeaderStoreReader, D: RelationsStoreReader, R: ReachabilityStoreReader + Clone>
    ConflictZoneManager<C, O, D, R>
{
    pub fn new(
        k: KType,
        root: Hash,
        dagknight_store: Arc<C>,
        headers_store: Arc<O>,
        relations_store: FutureIntersectRelations<D, MTReachabilityService<R>>,
        reachability_service: MTReachabilityService<R>,
    ) -> Self {
        Self { k, root, free_search: false, dagknight_store, headers_store, reachability_service, relations_store }
    }

    pub fn with_free_search(
        k: KType,
        root: Hash,
        dagknight_store: Arc<C>,
        headers_store: Arc<O>,
        relations_store: FutureIntersectRelations<D, MTReachabilityService<R>>,
        reachability_service: MTReachabilityService<R>,
        free_search: bool,
    ) -> Self {
        Self { k, root, free_search, dagknight_store, headers_store, reachability_service, relations_store }
    }

    pub fn has(&self, pov_hash: Hash) -> bool {
        let key = self.get_key(pov_hash);

        self.dagknight_store.has(key).unwrap()
    }

    pub fn insert(&self, pov_hash: Hash, gd: Arc<GhostdagData>) -> Result<(), StoreError> {
        let key = self.get_key(pov_hash);

        self.dagknight_store.insert(key, gd)
    }

    fn get_key(&self, pov_hash: Hash) -> DagknightKey {
        DagknightKey::new(self.root, pov_hash, self.k, self.free_search)
    }

    pub fn get_blue_score(&self, pov_hash: Hash) -> Result<u64, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.blue_score)
    }

    pub fn get_blue_work(&self, pov_hash: Hash) -> Result<BlueWorkType, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.blue_work)
    }

    pub fn get_selected_parent(&self, pov_hash: Hash) -> Result<Hash, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.selected_parent)
    }

    pub fn get_blues_anticone_sizes(&self, pov_hash: Hash) -> Result<Arc<BlockHashMap<KType>>, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.blues_anticone_sizes.clone())
    }

    pub fn get_data(&self, pov_hash: Hash) -> Result<Arc<GhostdagData>, StoreError> {
        let key = self.get_key(pov_hash);

        self.dagknight_store.get_data(key)
    }

    pub fn k_colouring(&self, parents: &[Hash], k: KType, custom_selected_parent: Option<Hash>) -> GhostdagData {
        assert!(!parents.is_empty(), "genesis must be added via a call to init");

        // Run the GHOSTDAG parent selection algorithm
        let selected_parent = custom_selected_parent.unwrap_or(self.find_selected_parent(parents.iter().copied()));
        // Handle the special case of origin children first
        if selected_parent.is_origin() {
            // ORIGIN is always a single parent so both blue score and work should remain zero
            return GhostdagData::new_with_selected_parent(selected_parent, 1); // k is only a capacity hint here
        }
        // Initialize new GHOSTDAG block data with the selected parent
        let mut new_block_data = GhostdagData::new_with_selected_parent(selected_parent, k);
        // Get the mergeset in consensus-agreed topological order (topological here means forward in time from blocks to children)
        let ordered_mergeset = self.ordered_mergeset_without_selected_parent(selected_parent, parents);

        for blue_candidate in ordered_mergeset.iter().cloned() {
            let coloring = self.check_blue_candidate(&new_block_data, blue_candidate, k);

            if let ColoringOutput::Blue(blue_anticone_size, blues_anticone_sizes) = coloring {
                // No k-cluster violation found, we can now set the candidate block as blue
                new_block_data.add_blue(blue_candidate, blue_anticone_size, &blues_anticone_sizes);
            } else {
                new_block_data.add_red(blue_candidate);
            }
        }

        let blue_score = self.get_blue_score(selected_parent).unwrap() + new_block_data.mergeset_blues.len() as u64;

        let added_blue_work: BlueWorkType =
            new_block_data.mergeset_blues.iter().cloned().map(|hash| calc_work(self.headers_store.get_bits(hash).unwrap())).sum();
        let blue_work: BlueWorkType = self.get_blue_work(selected_parent).unwrap() + added_blue_work;

        new_block_data.finalize_score_and_work(blue_score, blue_work);

        new_block_data
    }

    fn check_blue_candidate_with_chain_block(
        &self,
        new_block_data: &GhostdagData,
        chain_block: &ChainBlock,
        blue_candidate: Hash,
        candidate_blues_anticone_sizes: &mut BlockHashMap<KType>,
        candidate_blue_anticone_size: &mut KType,
        k: KType,
    ) -> ColoringState {
        // If blue_candidate is in the future of chain_block, it means
        // that all remaining blues are in the past of chain_block and thus
        // in the past of blue_candidate. In this case we know for sure that
        // the anticone of blue_candidate will not exceed K, and we can mark
        // it as blue.
        //
        // The new block is always in the future of blue_candidate, so there's
        // no point in checking it.

        // We check if chain_block is not the new block by checking if it has a hash.
        if let Some(hash) = chain_block.hash
            && self.reachability_service.is_dag_ancestor_of(hash, blue_candidate)
        {
            return ColoringState::Blue;
        }

        // Iterate over blue peers and check for k-cluster violations
        for &peer in chain_block.data.mergeset_blues.iter() {
            // Skip blocks that are in the past of blue_candidate (since they are not in its anticone)
            if self.reachability_service.is_dag_ancestor_of(peer, blue_candidate) {
                continue;
            }

            // Otherwise, peer must be in the anticone of blue_candidate, so we check for k limits.
            // Note that peer cannot be in the future of blue_candidate because we process the mergeset
            // in past-to-future topological order, so even if chain_block == new_block, an existing blue
            // cannot be in the future of a candidate blue

            let peer_blue_anticone_size = self.blue_anticone_size(peer, new_block_data);
            candidate_blues_anticone_sizes.insert(peer, peer_blue_anticone_size);

            *candidate_blue_anticone_size += 1;
            if *candidate_blue_anticone_size > k {
                // k-cluster violation: The candidate's blue anticone exceeded k
                return ColoringState::Red;
            }

            if peer_blue_anticone_size == k {
                // k-cluster violation: A block in candidate's blue anticone already
                // has k blue blocks in its own anticone
                return ColoringState::Red;
            }

            // This is a sanity check that validates that a blue
            // block's blue anticone is not already larger than K.
            assert!(peer_blue_anticone_size <= k, "found blue anticone larger than K");
            // [Crescendo]: this ^ is a valid assert since we are increasing k. Had we decreased k, this line would
            //              need to be removed and the condition above would need to be changed to >= k
        }

        ColoringState::Pending
    }

    /// Returns the blue anticone size of `block` from the worldview of `context`.
    /// Expects `block` to be in the blue set of `context`
    fn blue_anticone_size(&self, block: Hash, context: &GhostdagData) -> KType {
        let mut current_blues_anticone_sizes = HashKTypeMap::clone(&context.blues_anticone_sizes);
        let mut current_selected_parent = context.selected_parent;
        loop {
            if let Some(size) = current_blues_anticone_sizes.get(&block) {
                return *size;
            }

            // if current_selected_parent == self.genesis_hash || current_selected_parent == blockhash::ORIGIN {
            //     panic!("block {block} is not in blue set of the given context");
            // }

            current_blues_anticone_sizes = self.get_blues_anticone_sizes(current_selected_parent).unwrap();
            current_selected_parent = self.get_selected_parent(current_selected_parent).unwrap();
        }
    }

    fn check_blue_candidate(&self, new_block_data: &GhostdagData, blue_candidate: Hash, k: KType) -> ColoringOutput {
        // The maximum length of new_block_data.mergeset_blues can be K+1 because
        // it contains the selected parent.
        if new_block_data.mergeset_blues.len() as KType == k + 1 {
            return ColoringOutput::Red;
        }

        let mut candidate_blues_anticone_sizes: BlockHashMap<KType> = BlockHashMap::with_capacity(k as usize);
        // Iterate over all blocks in the blue past of the new block that are not in the past
        // of blue_candidate, and check for each one of them if blue_candidate potentially
        // enlarges their blue anticone to be over K, or that they enlarge the blue anticone
        // of blue_candidate to be over K.
        let mut chain_block = ChainBlock { hash: None, data: new_block_data.into() };
        let mut candidate_blue_anticone_size: KType = 0;

        loop {
            let state = self.check_blue_candidate_with_chain_block(
                new_block_data,
                &chain_block,
                blue_candidate,
                &mut candidate_blues_anticone_sizes,
                &mut candidate_blue_anticone_size,
                k,
            );

            match state {
                ColoringState::Blue => return ColoringOutput::Blue(candidate_blue_anticone_size, candidate_blues_anticone_sizes),
                ColoringState::Red => return ColoringOutput::Red,
                ColoringState::Pending => (), // continue looping
            }

            chain_block = ChainBlock {
                hash: Some(chain_block.data.selected_parent),
                data: self.get_data(chain_block.data.selected_parent).unwrap().into(),
            }
        }
    }

    fn sort_blocks(&self, blocks: impl IntoIterator<Item = Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<Hash> = blocks.into_iter().collect();
        sorted_blocks.sort_by_cached_key(|block| SortableBlock {
            hash: *block,
            blue_work: self.headers_store.get_header(*block).unwrap().blue_work,
        });
        sorted_blocks
    }

    pub fn ordered_mergeset_without_selected_parent(&self, selected_parent: Hash, parents: &[Hash]) -> Vec<Hash> {
        self.sort_blocks(self.unordered_mergeset_without_selected_parent(selected_parent, parents))
    }

    pub fn unordered_mergeset_without_selected_parent(&self, selected_parent: Hash, parents: &[Hash]) -> BlockHashSet {
        unordered_mergeset_without_selected_parent(&self.relations_store, &self.reachability_service, selected_parent, parents)
    }

    pub fn is_free_search(&self) -> bool {
        self.free_search
    }

    pub fn find_selected_parent(&self, parents: impl IntoIterator<Item = Hash>) -> Hash {
        let selected_parent = parents
            .into_iter()
            .filter_map(|parent| self.get_blue_work(parent).map(|blue_work| SortableBlock { hash: parent, blue_work }).ok())
            .max()
            .unwrap()
            .hash;

        if !self.free_search {
            assert!(
                self.reachability_service.is_chain_ancestor_of(self.root, selected_parent),
                "conflict genesis {} not a chain ancestor of selected parent {}",
                self.root,
                selected_parent
            );
        };

        selected_parent
    }

    pub fn init_root(&self) {
        if !self.has(self.root) {
            self.insert(
                self.root,
                Arc::new(GhostdagData::new(
                    0,
                    Default::default(),
                    blockhash::ORIGIN,
                    BlockHashes::new(Vec::new()),
                    BlockHashes::new(Vec::new()),
                    HashKTypeMap::new(BlockHashMap::new()),
                )),
            )
            .idempotent()
            .unwrap();
        }
    }

    /// Finds the known-colouring boundary blocks in the inclusive DAG past of `tips`.
    ///
    /// The traversal walks backward through blocks without stored DK colouring data for this (`root`, `k`, search mode)
    /// context and stops each explored parent path at the first block for which such data exists.
    ///
    /// In free-search mode, `next_chain_ancestor` must be `None`. In committed-search mode, it must be `Some(nca)`, and the
    /// traversal is restricted to the inclusive chain future of `nca`. The conflict genesis is exempt because it is the last
    /// possible known boundary of the backward traversal.
    ///
    /// Returns the known boundary blocks and all visited blocks. The latter includes blocks rejected by committed-search
    /// filtering.
    pub fn find_known_ancestor_boundary_blocks(&self, tips: &[Hash], next_chain_ancestor: Option<Hash>) -> (Vec<Hash>, BlockHashSet) {
        assert_eq!(self.free_search, next_chain_ancestor.is_none(), "free search expects no NCA. committed search expects an NCA");

        let mut visited = BlockHashSet::new();
        let mut queue: VecDeque<Hash> = VecDeque::from_iter(
            tips.iter()
                .filter(|&&t| next_chain_ancestor.is_none_or(|nca| self.reachability_service.is_chain_ancestor_of(nca, t)))
                .copied(),
        );

        let mut known_boundary_blocks = vec![];

        while let Some(curr) = queue.pop_front() {
            if !visited.insert(curr) {
                continue;
            }

            // In committed search, prune blocks outside the subgroup's inclusive NCA chain future.
            // Keep the conflict genesis as the last possible known boundary of the backward traversal.
            if !self.free_search
                && self.root != curr
                && !next_chain_ancestor.is_none_or(|nca| self.reachability_service.is_chain_ancestor_of(nca, curr))
            {
                continue;
            }

            if self.has(curr) {
                known_boundary_blocks.push(curr);
            } else {
                for &parent in self.relations_store.get_parents(curr).unwrap().iter() {
                    queue.push_back(parent);
                }
            }
        }

        (known_boundary_blocks, visited)
    }

    // Calculates the rank of the subgroup over the region: <root, tips>
    // root = conflict genesis
    // subgroup = the current subgroup
    // tips = all tips in this conflict. part of which is the subgroup
    //
    // `next_chain_ancestor`:
    //   - If `free_search` is true, this must be `None` (asserted).
    //   - If `free_search` is false, this must be `Some(nca)` where `nca` is a block whose selected
    //     parent is `root` (asserted). This bounds the zone fill to the subgroup's region.
    //
    // Returns the conflict zone manager which gives access to the coloring data of the conflict zone
    pub fn fill_zone_data(&self, tips: &[Hash], next_chain_ancestor: Option<Hash>) -> BlockHashSet {
        // Construct lock key and validate parameters
        let lock_key = if self.free_search {
            assert!(next_chain_ancestor.is_none(), "free_search expects None for next_chain_ancestor");
            KColouringLockKey::free_search_key(self.root, self.k)
        } else {
            let nca = next_chain_ancestor.expect("committed_search expects Some(next_chain_ancestor)");
            // Assert NCA's selected parent is self.root (conflict_genesis)
            assert!(
                self.reachability_service.is_chain_ancestor_of(self.root, nca),
                "conflict_genesis must be a chain ancestor of next_chain_ancestor"
            );
            KColouringLockKey::committed_search_key(self.root, self.k, nca)
        };

        // Acquire lock for this zone fill
        let locks = get_k_colouring_locks();
        let lock_arc = locks.entry(lock_key).or_insert_with(|| Arc::new(RwLock::new(()))).clone();
        let _guard = lock_arc.write();

        // populate dummy root to DKStore
        self.init_root();

        let (known_ancestor_boundary_blocks, visited_subdag) = self.find_known_ancestor_boundary_blocks(tips, next_chain_ancestor);

        let mut topological_heap: BinaryHeap<_> = Default::default();

        known_ancestor_boundary_blocks.iter().for_each(|current_root| {
            topological_heap.push(Reverse(SortableBlock {
                hash: *current_root,
                blue_work: self.headers_store.get_header(*current_root).unwrap().blue_work,
            }));
        });

        let mut visited = BlockHashSet::new();

        loop {
            let Some(current) = topological_heap.pop() else {
                break;
            };
            let current_hash = current.0.hash;
            if !visited.insert(current_hash) {
                continue;
            }

            if !self.reachability_service.is_dag_ancestor_of_any(current_hash, &mut tips.iter().copied()) {
                continue;
            }

            if !self.has(current_hash) {
                let parents = &self.relations_store.get_parents(current_hash).unwrap();

                // For free_search, select from all parents; for committed search, only from agreeing parents
                let selected_parent = if self.free_search {
                    self.find_selected_parent(parents.iter().copied())
                } else {
                    let next_chain_ancestor_of_current = next_chain_ancestor.unwrap();
                    let agreeing_parents = parents
                        .iter()
                        .copied()
                        .filter(|&p| {
                            next_chain_ancestor_of_current == current_hash
                                || self.reachability_service.is_chain_ancestor_of(next_chain_ancestor_of_current, p)
                        })
                        .collect::<Vec<_>>();

                    // sanity checks - start
                    assert!(
                        !agreeing_parents.is_empty(),
                        "Expected at least one agreeing parent | current: {:#?} | parents: {:#?}",
                        current_hash,
                        parents
                    );

                    // all parents here must already exist assuming topological sorting is honored
                    // so finding one that doesn't means an error in processing and must be diagnosed
                    if let Some(&parent) = agreeing_parents
                        .iter()
                        .filter(|&&parent| !self.has(parent))
                        .filter(|&&parent| tips.iter().any(|&tip| self.reachability_service.is_chain_ancestor_of(parent, tip)))
                        .next()
                    {
                        known_ancestor_boundary_blocks
                            .iter()
                            .filter(|&&boundary_block| self.reachability_service.is_dag_ancestor_of(parent, boundary_block))
                            .for_each(|&boundary_block| {
                                println!(
                                    "cg: {} | k: {} | fs: {} | nca: {:?} | parent {} is in the past of known boundary block {}",
                                    self.root, self.k, self.free_search, next_chain_ancestor, parent, boundary_block
                                );
                            });
                        panic!(
                            "cg: {} | k: {} | fs: {} | nca: {:?} | known_ancestor_boundary_blocks: {:?} | Expected agreeing parent to have coloring data | current: {:#?} | missing_parent: {:#?} | curr_parents: {:#?} | tips: {:?}",
                            self.root,
                            self.k,
                            self.free_search,
                            next_chain_ancestor_of_current,
                            known_ancestor_boundary_blocks,
                            current_hash,
                            parent,
                            parents,
                            tips
                        );
                    };

                    if !agreeing_parents.iter().any(|&parent| self.has(parent)) {
                        panic!(
                            "cg: {} | k: {} | fs: {} | nca: {:?} | known_ancestor_boundary_blocks: {:?} | no agreeing parent with data | current: {:#?} | agreeing_parents: {:#?} | tips: {:?}",
                            self.root,
                            self.k,
                            self.free_search,
                            next_chain_ancestor_of_current,
                            known_ancestor_boundary_blocks,
                            current_hash,
                            agreeing_parents,
                            tips
                        );
                    }
                    // sanity checks - end

                    self.find_selected_parent(agreeing_parents.iter().copied())
                };

                let current_gd = self.k_colouring(parents, self.k, Some(selected_parent));

                self.insert(current_hash, Arc::new(current_gd)).idempotent().unwrap();
            }

            for child in self.relations_store.get_children(current_hash).unwrap().read().iter().copied() {
                // For free_search, use DAG ancestry; for committed search, use chain ancestry
                let is_in_zone = if self.free_search {
                    self.reachability_service.try_is_dag_ancestor_of(self.root, child).unwrap_or(false)
                } else {
                    self.reachability_service.try_is_chain_ancestor_of(next_chain_ancestor.unwrap(), child).unwrap_or(false)
                };
                if !is_in_zone {
                    continue;
                }
                topological_heap
                    .push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
            }
        }

        // Opportunistically cleanup unused locks after zone fill
        cleanup_k_colouring_locks();

        visited_subdag
    }
    // END Copied from GD Manager
}

impl<C: DagknightStore + DagknightStoreReader, O: HeaderStoreReader, D: RelationsStoreReader, R: ReachabilityStoreReader + Clone>
    ColoringReader for ConflictZoneManager<C, O, D, R>
{
    fn get_coloring_data(&self, hash: Hash) -> Arc<GhostdagData> {
        self.get_data(hash).expect("zone coloring data missing for a chain block that was filled")
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use super::*;
    use crate::model::stores::{
        dagknight::MemoryDagknightStore, headers::MemoryHeaderStore, reachability::MemoryReachabilityStore,
        relations::MemoryRelationsStore,
    };
    use crate::processes::reachability::tests::{DagBlock, DagBuilder};
    use kaspa_consensus_core::blockhash::ORIGIN;
    use kaspa_consensus_core::header::Header;
    use kaspa_math::Uint192;
    use parking_lot::RwLock;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::fs::File;
    use std::str::FromStr;

    #[test]
    fn test_k_colouring_lock_key_constructors() {
        let cg: Hash = 1_u64.into();
        let nca: Hash = 2_u64.into();
        let k = 5;

        let committed = KColouringLockKey::committed_search_key(cg, k, nca);
        assert_eq!(committed.conflict_genesis(), cg);
        assert!(!committed.is_free_search());

        let free = KColouringLockKey::free_search_key(cg, k);
        assert_eq!(free.conflict_genesis(), cg);
        // Free search key should use conflict_genesis as nca
        assert_eq!(free.nca, cg);
        assert!(free.is_free_search());
    }

    #[test]
    fn test_k_colouring_lock_key_equality() {
        let cg: Hash = 1_u64.into();
        let nca1: Hash = 2_u64.into();
        let nca2: Hash = 3_u64.into();
        let k1 = 5;
        let k2 = 10;

        let key1 = KColouringLockKey::committed_search_key(cg, k1, nca1);
        let key2 = KColouringLockKey::committed_search_key(cg, k1, nca1);
        assert_eq!(key1, key2);

        // Different NCA should yield different keys (allows concurrent processing of independent subgroups)
        let key3 = KColouringLockKey::committed_search_key(cg, k1, nca2);
        assert_ne!(key1, key3);

        // Different K should yield different keys
        let key4 = KColouringLockKey::committed_search_key(cg, k2, nca1);
        assert_ne!(key1, key4);

        // Free search key should differ from committed key with same CG and K
        let key5 = KColouringLockKey::free_search_key(cg, k1);
        assert_ne!(key1, key5);
    }

    #[test]
    fn test_get_k_colouring_locks_singleton() {
        let map1 = get_k_colouring_locks();
        let map2 = get_k_colouring_locks();
        assert!(std::ptr::eq(map1, map2), "get_k_colouring_locks should return the same singleton map");
    }

    #[test]
    fn test_cleanup_k_colouring_locks_removes_unused() {
        let map = get_k_colouring_locks();
        let key = KColouringLockKey::committed_search_key(100_u64.into(), 5, 101_u64.into());
        let lock = Arc::new(RwLock::new(()));
        map.insert(key.clone(), lock.clone());

        assert!(map.contains_key(&key), "Key should exist in map");

        drop(lock); // Now only 1 strong ref remains (the one in the map)
        cleanup_k_colouring_locks();
        assert!(!map.contains_key(&key), "Unused lock should be cleaned up");
    }

    #[test]
    fn test_cleanup_k_colouring_locks_keeps_used() {
        let map = get_k_colouring_locks();
        let key = KColouringLockKey::committed_search_key(200_u64.into(), 5, 201_u64.into());
        let lock = Arc::new(RwLock::new(()));
        map.insert(key.clone(), lock.clone());

        // Acquire the lock to simulate active usage
        let _guard = lock.read();
        assert!(map.contains_key(&key), "Key should exist in map");

        cleanup_k_colouring_locks();
        // Should still be present because `_guard` holds an active reference
        assert!(map.contains_key(&key), "Active lock should be preserved");

        drop(_guard);
        drop(lock); // Drop our reference so only the map's reference remains
        cleanup_k_colouring_locks();
        assert!(!map.contains_key(&key), "Lock should be cleaned up after all external references are dropped");
    }

    /// Test that `find_known_ancestor_boundary_blocks` correctly uses chain ancestry (committed) vs DAG ancestry (free
    /// search) when traversing back from tips.
    ///
    /// DAG structure:
    ///
    ///         A <= B <= D -- F
    ///          \           /
    ///            \       /
    ///        Z <- C <= E -- W
    ///         \    \      /
    ///           \   \   /
    ///            Y <= X
    ///
    /// Selected parents:
    /// - A: ORIGIN, B: A, D: B, Z: ORIGIN
    /// - C: A (agrees with A), E: C
    /// - Y: Z, X: Y (X does NOT agree with A - its chain goes X→Y→Z→ORIGIN)
    /// - F, W: tips without colouring records
    ///
    /// Parents:
    /// - A:[ORIGIN], B:[A], D:[B], F:[D,E]
    /// - Z:[ORIGIN], C:[A,Z], E:[C], W:[X,E]
    /// - Y:[Z], X:[Y,C]
    ///
    /// Chain ancestry from A: A→B→D and A→C→E
    /// A is not a chain ancestor of X
    ///
    /// Records filled for: A, B, C, D, E, Y, Z, X
    /// F and W are tips (no records)
    ///
    /// TEST with tips = [F, W]:
    /// - committed search returns [D, E];
    /// - free search returns [D, E, X].
    #[test]
    fn test_find_known_ancestor_boundary_blocks_respects_search_mode() {
        let hash_a: Hash = 1_u64.into(); // root
        let hash_b: Hash = 2_u64.into();
        let hash_d: Hash = 3_u64.into();
        let hash_z: Hash = 4_u64.into();
        let hash_c: Hash = 5_u64.into();
        let hash_e: Hash = 6_u64.into();
        let hash_y: Hash = 7_u64.into();
        let hash_x: Hash = 8_u64.into();
        let hash_f: Hash = 9_u64.into();
        let hash_w: Hash = 10_u64.into();

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map.clone()));
        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();

        // Build DAG
        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations);
            builder.init();
            builder.add_block(DagBlock::new(hash_a, vec![ORIGIN]));
            builder.add_block(DagBlock::new(hash_z, vec![ORIGIN]));
            builder.add_block_with_selected_parent(DagBlock::new(hash_b, vec![hash_a]), hash_a);
            builder.add_block_with_selected_parent(DagBlock::new(hash_c, vec![hash_a, hash_z]), hash_a);
            builder.add_block_with_selected_parent(DagBlock::new(hash_d, vec![hash_b]), hash_b);
            builder.add_block_with_selected_parent(DagBlock::new(hash_e, vec![hash_c]), hash_c);
            builder.add_block_with_selected_parent(DagBlock::new(hash_y, vec![hash_z]), hash_z);
            builder.add_block_with_selected_parent(DagBlock::new(hash_x, vec![hash_y, hash_c]), hash_y);
            builder.add_block(DagBlock::new(hash_f, vec![hash_d, hash_e])); // Tip
            builder.add_block(DagBlock::new(hash_w, vec![hash_x, hash_e])); // Tip

            // Insert headers with valid bits
            for (hash, parents) in [
                (hash_a, vec![]),
                (hash_b, vec![hash_a]),
                (hash_d, vec![hash_b]),
                (hash_z, vec![]),
                (hash_c, vec![hash_a, hash_z]),
                (hash_e, vec![hash_c]),
                (hash_y, vec![hash_z]),
                (hash_x, vec![hash_y, hash_c]),
                (hash_f, vec![hash_d, hash_e]),
                (hash_w, vec![hash_x, hash_e]),
            ] {
                let mut header = Header::from_precomputed_hash(hash, parents);
                header.bits = 0x207fffff;
                headers_store.insert(Arc::new(header));
            }
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let relations_service = FutureIntersectRelations::new(relations.clone(), reachability_service.clone(), hash_a);

        // Create both managers sharing the same stores
        let manager_committed = ConflictZoneManager::new(
            0,
            hash_a,
            dagknight_store.clone(),
            headers_store.clone(),
            relations_service.clone(),
            reachability_service.clone(),
        );

        let manager_free = ConflictZoneManager::with_free_search(
            0,
            hash_a,
            dagknight_store.clone(),
            headers_store.clone(),
            relations_service,
            reachability_service,
            true,
        );

        // Initialize root and fill records for all blocks except tips F and W
        manager_committed.init_root();
        manager_free.init_root();

        // Fill records for non-tip blocks
        for (hash, selected_parent) in [
            (hash_b, hash_a),
            (hash_d, hash_b),
            (hash_z, ORIGIN),
            (hash_c, hash_a),
            (hash_e, hash_c),
            (hash_y, hash_z),
            (hash_x, hash_y),
        ] {
            let gd = GhostdagData::new_with_selected_parent(selected_parent, 0);
            manager_committed.insert(hash, Arc::new(gd.clone())).unwrap();
            manager_free.insert(hash, Arc::new(gd)).unwrap();
        }

        // Tips are F and W (no records yet)
        let tips = vec![hash_f, hash_w];

        let (boundary_blocks_committed, _) = manager_committed.find_known_ancestor_boundary_blocks(&tips, None);
        let (boundary_blocks_free, _) = manager_free.find_known_ancestor_boundary_blocks(&tips, None);

        assert_eq!(boundary_blocks_committed.len(), 2, "Committed should find D, E");
        assert!(boundary_blocks_committed.contains(&hash_d));
        assert!(boundary_blocks_committed.contains(&hash_e));
        assert!(!boundary_blocks_committed.contains(&hash_x), "X should not be in committed boundary blocks");

        assert_eq!(boundary_blocks_free.len(), 3, "Free search should find D, E, X");
        assert!(boundary_blocks_free.contains(&hash_d));
        assert!(boundary_blocks_free.contains(&hash_e));
        assert!(boundary_blocks_free.contains(&hash_x));
    }

    /// Test demonstrating the key difference between free_search and committed search.
    ///
    /// DAG structure:
    ///
    ///        A (conflict genesis)
    ///       / \
    ///      B   Z
    ///      |   |
    ///      C   Y
    ///      | \ |
    ///      D   X
    ///
    ///
    /// The tips are [D, X].
    ///
    /// TEST: When computing X's selected_parent during fill_zone_data:
    /// - In free_search=false (committed): selected_parent must be Y
    ///   (X only agrees with Y, not with C - they don't share a chain ancestor above A)
    /// - In free_search=true: selected_parent considers all parents [Y, C]
    ///   and selects based on blue work (or hash as tiebreaker). In this case, C wins
    #[test]
    fn test_free_search_considers_non_agreeing_parents() {
        use crate::processes::reachability::tests::{DagBlock, DagBuilder};

        let hash_a: Hash = 1_u64.into(); // conflict genesis
        let hash_b: Hash = 2_u64.into();
        let hash_c: Hash = 3_u64.into();
        let hash_d: Hash = 4_u64.into();
        let hash_z: Hash = 5_u64.into();
        let hash_y: Hash = 6_u64.into();
        let hash_x: Hash = 7_u64.into();

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));

        let headers_store = Arc::new(MemoryHeaderStore::new());

        let mut reachability = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();

        // Build DAG for committed search
        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations_store);
            builder.init();
            builder.add_block(DagBlock::new(hash_a, vec![ORIGIN]));
            builder.add_block_with_selected_parent(DagBlock::new(hash_b, vec![hash_a]), hash_a);
            builder.add_block_with_selected_parent(DagBlock::new(hash_c, vec![hash_b]), hash_b);
            builder.add_block_with_selected_parent(DagBlock::new(hash_d, vec![hash_c]), hash_c);
            builder.add_block_with_selected_parent(DagBlock::new(hash_z, vec![hash_a]), hash_a);
            builder.add_block_with_selected_parent(DagBlock::new(hash_y, vec![hash_z]), hash_z);
            builder.add_block_with_selected_parent(DagBlock::new(hash_x, vec![hash_y, hash_c]), hash_y);

            let insert_header_with_work =
                |hash: Hash, parents: Vec<Hash>, bits: u32, store: &Arc<MemoryHeaderStore>, blue_work: BlueWorkType| {
                    let mut header = Header::from_precomputed_hash(hash, parents);
                    header.bits = bits;
                    header.blue_work = blue_work;
                    store.insert(Arc::new(header));
                };

            insert_header_with_work(hash_a, vec![], 0x207fffff, &headers_store, 0.into());
            // Note the higher bits here to make this side have higher blue work, but not be the committed side
            insert_header_with_work(hash_b, vec![hash_a], 0x204fffff, &headers_store, 1.into());
            insert_header_with_work(hash_c, vec![hash_b], 0x207fffff, &headers_store, 3.into());
            insert_header_with_work(hash_d, vec![hash_b], 0x207fffff, &headers_store, 4.into());

            insert_header_with_work(hash_z, vec![hash_a], 0x207fffff, &headers_store, 1.into());
            insert_header_with_work(hash_y, vec![hash_z], 0x207fffff, &headers_store, 2.into());
            insert_header_with_work(hash_x, vec![hash_c, hash_y], 0x207fffff, &headers_store, 6.into());
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let relations_service = FutureIntersectRelations::new(relations_store.clone(), reachability_service.clone(), hash_a);

        // Create committed manager (free_search = false)
        let manager_committed = ConflictZoneManager::new(
            0,
            hash_a,
            dagknight_store.clone(),
            headers_store.clone(),
            relations_service.clone(),
            reachability_service.clone(),
        );

        // Create free search manager (free_search = true)
        let manager_free = ConflictZoneManager::with_free_search(
            0,
            hash_a,
            dagknight_store,
            headers_store,
            relations_service,
            reachability_service,
            true,
        );
        assert!(manager_free.is_free_search(), "Manager should have free_search=true");

        // Pre-populate the store with blocks (simulating that they were already processed)
        // For committed search
        manager_committed.init_root();

        // Now fill zone data
        let tips = vec![hash_x, hash_d];
        // For committed search, NCA is hash_z (whose selected parent is hash_a, the conflict genesis)
        manager_committed.fill_zone_data(&tips, Some(hash_z));
        // For free search, NCA is None
        manager_free.fill_zone_data(&tips, None);

        // Get X's selected parent from both managers
        let committed_sp = manager_committed.get_selected_parent(hash_x).unwrap();
        let free_sp = manager_free.get_selected_parent(hash_x).unwrap();

        assert_eq!(committed_sp, hash_y, "In committed search, X's selected parent must be Y (the only agreeing parent)");

        // In free search, X can select any parent and is expected to select C due to higher blue work (even if not agreeing)
        assert_eq!(
            free_sp, hash_c,
            "In free search, X's selected parent should be C (selected from all parents, wins by higher work)"
        );
    }

    #[test]
    fn test_czm_lkt_correctness() {
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let headers_store = Arc::new(MemoryHeaderStore::new());

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map.clone()));

        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();

        let mut add_block = |hash, parents: Vec<Hash>, blue_work, bits, blue_score, daa_score, selected_parent: Hash| -> Hash {
            let mut header = Header::from_precomputed_hash(hash, parents.clone());
            header.bits = bits;
            header.blue_work = blue_work;
            header.blue_score = blue_score;
            header.daa_score = daa_score;
            headers_store.insert(Arc::new(header));

            builder.add_block_with_selected_parent(DagBlock::new(hash, parents.clone()), selected_parent);
            hash
        };

        let json_filename = "test_conflict_lkt.json";
        let file = File::open(json_filename).expect("Unable to open JSON file");
        let json_data: serde_json::Value = serde_json::from_reader(file).expect("Unable to parse JSON");

        let tips: Vec<Hash> =
            json_data["tips"].as_array().unwrap().iter().map(|t| Hash::from_str(t.as_str().unwrap()).unwrap()).collect();

        let blocks = json_data["blocks"].as_array().expect("Blocks is not an array");

        let test_blocks: Vec<(Hash, Vec<Hash>, Uint192, u32, u64, u64, Hash)> = blocks
            .iter()
            .map(|block| {
                let id = Hash::from_str(block["id"].as_str().unwrap()).unwrap();
                let parents: Vec<Hash> = if block["parents"].as_array().map(|a| a.is_empty()).unwrap_or(false) {
                    vec![ORIGIN]
                } else {
                    block["parents"].as_array().unwrap().iter().map(|p| Hash::from_str(p.as_str().unwrap()).unwrap()).collect()
                };
                let blue_work = Uint192::from_u64(block["blue_work"].as_str().unwrap().parse::<u64>().unwrap());
                let bits = u32::from_str_radix(block["bits"].as_str().unwrap(), 16).unwrap();
                let blue_score = u64::from_str(block["blue_score"].as_str().unwrap()).unwrap();
                let daa_score = u64::from_str(block["daa_score"].as_str().unwrap()).unwrap();
                let selected_parent = if block["selected_parent"].is_null() {
                    ORIGIN
                } else {
                    Hash::from_str(block["selected_parent"].as_str().unwrap()).unwrap()
                };
                (id, parents, blue_work, bits, blue_score, daa_score, selected_parent)
            })
            .collect();

        let mut test_blocks = test_blocks;

        test_blocks.sort_by_key(|(_, _, blue_work, _, _, _, _)| *blue_work);

        for (hash, parents, blue_work, bits, blue_score, daa_score, selected_parent) in &test_blocks {
            add_block(*hash, parents.clone(), *blue_work, *bits, *blue_score, *daa_score, *selected_parent);
        }

        // TODO: Intantiate CZM and run the fills
        let conflict_genesis = Hash::from_str("057554b30009254cc93b2bbadc84d084b148edfe65d58a3b95476062c65b129d").unwrap();
        let nca_1 = Hash::from_str("0770d603d43a88586e76385d3f15b0d74152a726f772a465183172dcf5e02e0b").unwrap();
        let nca_2 = Hash::from_str("85fb472d19eed6ebad22a4d8135102c6436fbda81a002b0805852f0ba43d2363").unwrap();

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let fir_relations = FutureIntersectRelations::new(relations, reachability_service.clone(), conflict_genesis);

        let czm =
            ConflictZoneManager::new(1, conflict_genesis, dagknight_store, headers_store.clone(), fir_relations, reachability_service);

        // let tips = vec![];
        println!("boundary base: {:?}", czm.find_known_ancestor_boundary_blocks(&tips, Some(nca_2)).0);
        czm.fill_zone_data(
            &vec![Hash::from_str("b2c22e6c802483e51e37d22a782a5a98379f39328618780e96d195eefbfa9f3e").unwrap()],
            Some(nca_2),
        );
        println!("boundary before nca_1: {:?}", czm.find_known_ancestor_boundary_blocks(&tips, Some(nca_1)).0);
        czm.fill_zone_data(&tips, Some(nca_1));
        println!("boundary after nca_1: {:?}", czm.find_known_ancestor_boundary_blocks(&tips, Some(nca_1)).0);
        println!("boundary before nca_2: {:?}", czm.find_known_ancestor_boundary_blocks(&tips, Some(nca_2)).0);
        czm.fill_zone_data(&tips, Some(nca_2));
        println!("boundary after nca_2: {:?}", czm.find_known_ancestor_boundary_blocks(&tips, Some(nca_2)).0);
    }

    #[test]
    fn test_czm_lkt_correctness_2() {
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let headers_store = Arc::new(MemoryHeaderStore::new());

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map.clone()));

        let json_filename = "test_captured_k5.json";
        let file = File::open(json_filename).expect("Unable to open JSON file");
        let json_data: serde_json::Value = serde_json::from_reader(file).expect("Unable to parse JSON");

        let tips: Vec<Hash> =
            json_data["tips"].as_array().unwrap().iter().map(|t| Hash::from_str(t.as_str().unwrap()).unwrap()).collect();
        let conflict_genesis = Hash::from_str(json_data["conflict_genesis"].as_str().unwrap()).unwrap();

        // (id, parents, blue_work, bits, blue_score, daa_score, selected_parent)
        let mut test_blocks: Vec<(Hash, Vec<Hash>, Uint192, u32, u64, u64, Option<Hash>)> = json_data["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|block| {
                let id = Hash::from_str(block["id"].as_str().unwrap()).unwrap();
                let parents: Vec<Hash> =
                    block["parents"].as_array().unwrap().iter().map(|p| Hash::from_str(p.as_str().unwrap()).unwrap()).collect();
                let blue_work = Uint192::from_u64(block["blue_work"].as_str().unwrap().parse::<u64>().unwrap());
                // bits may be "0x"-prefixed (captured zones) or bare hex
                let bits = u32::from_str_radix(block["bits"].as_str().unwrap().trim_start_matches("0x"), 16).unwrap();
                let blue_score = u64::from_str(block["blue_score"].as_str().unwrap()).unwrap();
                let daa_score = u64::from_str(block["daa_score"].as_str().unwrap()).unwrap();
                let selected_parent = block["selected_parent"].as_str().map(|s| Hash::from_str(s).unwrap());
                (id, parents, blue_work, bits, blue_score, daa_score, selected_parent)
            })
            .collect();

        // Stand-in blocks for selected parents that are referenced but not in the file
        let known: HashSet<Hash> = test_blocks.iter().map(|(id, ..)| *id).collect();
        let mut external_sps: Vec<Hash> = vec![];
        for sp in test_blocks.iter().filter_map(|(_, _, _, _, _, _, sp)| *sp) {
            if !known.contains(&sp) && !external_sps.contains(&sp) {
                external_sps.push(sp);
            }
        }
        for sp in external_sps {
            // copy the header fields of the first block that references this SP
            let (bw, bits, blue_score, daa_score) = test_blocks
                .iter()
                .find(|(_, _, _, _, _, _, sp_ref)| sp_ref.as_ref() == Some(&sp))
                .map(|(_, _, bw, bits, bs, ds, _)| (*bw, *bits, *bs, *ds))
                .expect("stand-in should have a referrer");
            test_blocks.push((sp, vec![], bw, bits, blue_score, daa_score, None));
        }

        test_blocks.sort_by_key(|(_, _, blue_work, _, _, _, _)| *blue_work);

        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();

        // Insert in topological order (a block only once all its parents are in)
        let mut remaining = test_blocks;
        let mut added: HashSet<Hash> = HashSet::new();
        while let Some(pos) = remaining.iter().position(|(_, parents, ..)| parents.iter().all(|p| p.is_origin() || added.contains(p)))
        {
            let (hash, mut parents, blue_work, bits, blue_score, daa_score, selected_parent) = remaining.remove(pos);
            if parents.is_empty() {
                parents.push(ORIGIN);
            }
            let chain_parent = match selected_parent {
                Some(sp) if parents.contains(&sp) => sp,
                Some(sp) => {
                    // SP filtered out of the zone's parent list: restore it
                    parents.push(sp);
                    sp
                }
                None if parents.len() == 1 => parents[0],
                None => parents.iter().max_by_key(|p| headers_store.get_header(**p).unwrap().blue_work).copied().unwrap(),
            };

            let mut header = Header::from_precomputed_hash(hash, parents.clone());
            header.bits = bits;
            header.blue_work = blue_work;
            header.blue_score = blue_score;
            header.daa_score = daa_score;
            headers_store.insert(Arc::new(header));

            builder.add_block_with_selected_parent(DagBlock::new(hash, parents), chain_parent);
            added.insert(hash);
        }
        assert!(remaining.is_empty(), "DAG is not acyclic: {remaining:?}");

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let nca_service = reachability_service.clone();
        let fir_relations = FutureIntersectRelations::new(relations, reachability_service.clone(), conflict_genesis);

        let czm =
            ConflictZoneManager::new(5, conflict_genesis, dagknight_store, headers_store.clone(), fir_relations, reachability_service);

        // The captured last-known tips; their LCA is the conflict genesis
        let lkt_tips: Vec<Hash> = [
            "7060636f8d0df73776bcb313bf1d82eaacb3e73019f992b160a8c399f8fc0668",
            "8e742298cd2c0b52fe3d15500ed22bb751fdbfbc3877004f63f5bb88b44d2f53",
            "ecff100eb4017c319a57536cdec0297cc4fcf0b62629cd704506eec7cdfbaf4f",
            "11b3b68baddf89943c302fed1d1a9cb6ab0a2ae6d977f68a67f74aee62caca34",
            "91015a3c99995f8eb65622833aba4ca8842604c28b5b5b1e714c03266f965020",
            "e834b5f79d3f9e626fb1c3dd773a7f2cf5a25268b5dfde61b6effcd9913e41f5",
            "320c2bd043a480994503b9d35abb85654e788a261d2f2c37dbb0184910f301fe",
            "57a75061912b083b0950b91fd517edd331b39148796254cfdfa1d76bcc6c5094",
        ]
        .iter()
        .map(|s| Hash::from_str(s).unwrap())
        .collect();

        // NCA of the single-tip (6fe29520…) zone subgroup
        let nca_2 = Hash::from_str("551db5c4501097ecc608815ac50b72c483d41d85419a2d82d48afab2b26ba325").unwrap();

        // Group the LKTs by NCA w.r.t. their LCA (= conflict genesis), as in protocol.rs
        let mut subgroups: Vec<(Hash, Vec<Hash>)> = Vec::new();
        for tip in &lkt_tips {
            let nca = nca_service.get_next_chain_ancestor(*tip, conflict_genesis);
            match subgroups.iter_mut().find(|(n, _)| *n == nca) {
                Some((_, group)) => group.push(*tip),
                None => subgroups.push((nca, vec![*tip])),
            }
        }
        for (nca, group) in &subgroups {
            println!("subgroup: nca={} tips={:?}", nca, group);
        }

        let (lkt_boundary, zone_boundary) = boundary_snapshot(&czm, &lkt_tips, &tips, nca_2);
        println!("boundary base: lkt={:?} zone={:?}", lkt_boundary, zone_boundary);

        for (nca, subgroup) in &subgroups {
            println!("fill subgroup: nca={} tips={}", nca, subgroup.len());
            czm.fill_zone_data(subgroup, Some(*nca));
            let (lkt_boundary, zone_boundary) = boundary_snapshot(&czm, &lkt_tips, &tips, nca_2);
            println!("boundary after subgroup fill: lkt={:?} zone={:?}", lkt_boundary, zone_boundary);
        }

        println!("fill all zone tips: nca={}", nca_2);
        czm.fill_zone_data(&tips, Some(nca_2));
        let (lkt_boundary, zone_boundary) = boundary_snapshot(&czm, &lkt_tips, &tips, nca_2);
        println!("boundary after all-tips fill: lkt={:?} zone={:?}", lkt_boundary, zone_boundary);
    }

    fn boundary_snapshot<
        C: DagknightStore + DagknightStoreReader,
        O: HeaderStoreReader,
        D: RelationsStoreReader,
        R: ReachabilityStoreReader + Clone,
    >(
        czm: &ConflictZoneManager<C, O, D, R>,
        lkt_tips: &[Hash],
        zone_tips: &[Hash],
        nca: Hash,
    ) -> (Vec<Hash>, Vec<Hash>) {
        (
            czm.find_known_ancestor_boundary_blocks(lkt_tips, Some(nca)).0,
            czm.find_known_ancestor_boundary_blocks(zone_tips, Some(nca)).0,
        )
    }
}
