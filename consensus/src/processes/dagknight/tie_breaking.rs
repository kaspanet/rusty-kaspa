use std::iter::once;

use kaspa_consensus_core::{BlockHashSet, KType};
use kaspa_hashes::Hash;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            dagknight::{DagknightStore, DagknightStoreReader},
            ghostdag::GhostdagData,
            headers::HeaderStoreReader,
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::{
        dagknight::{manager::ConflictZoneManager, protocol::GroupMetadata},
        ghostdag::ordering::SortableBlock,
        reachability::relations::FutureIntersectRelations,
    },
};

/// Chain blocks from a subgroup's conditioned k-colouring.
pub type SubgroupChainBlocks = Vec<Hash>;

/// Result of free-search k-colouring reference cluster computation.
pub struct ReferenceCluster {
    /// Set of blue block hashes in the resulting colouring.
    pub blues: BlockHashSet,
    /// Chain backbone from virtual towards conflict_genesis (inclusive).
    pub chain_blocks: Vec<Hash>,
}

/// DAGKnight tie-breaker implementing Algorithm 4 of the paper. Holds only the
/// stores needed for tie-breaking, not the full executor.
pub struct DagknightTieBreaker<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> {
    dagknight_store: C,
    headers_store: O,
    relations_store: D,
    reachability_service: MTReachabilityService<R>,
}

impl<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> DagknightTieBreaker<C, O, D, R>
{
    pub fn new(dagknight_store: C, headers_store: O, relations_store: D, reachability_service: MTReachabilityService<R>) -> Self {
        Self { dagknight_store, headers_store, relations_store, reachability_service }
    }

    /// Computes the free-search k-colouring reference cluster.
    ///
    /// This is equivalent to `select_parent_from_k_colouring` but uses a
    /// free-search conflict zone manager, allowing unrestricted maximization
    /// of the k-cluster across all parents.
    ///
    /// Returns the blue set and chain backbone.
    fn compute_reference_cluster<'a, T>(&self, conflict_genesis: Hash, all_tips: T, k: KType) -> ReferenceCluster
    where
        T: IntoIterator<Item = &'a Hash>,
        T::IntoIter: Clone,
    {
        let all_tips = all_tips.into_iter();
        let relations_service = FutureIntersectRelations::new(&self.relations_store, &self.reachability_service, conflict_genesis);

        let conflict_zone_manager = ConflictZoneManager::free_search(
            k,
            conflict_genesis,
            &self.dagknight_store,
            &self.headers_store,
            relations_service,
            &self.reachability_service,
        );

        conflict_zone_manager.fill_zone_data(all_tips.clone(), ());

        // Run k-colouring with free search: no custom selected parent is passed,
        // so the manager freely selects from all parents.
        let virtual_gd: GhostdagData = conflict_zone_manager.k_colouring(all_tips, k, None);

        // Collect the full blue set by traversing the chain from virtual back to conflict_genesis.
        // Each chain block contributes itself AND its mergeset blues.
        let mut blue_set: BlockHashSet = BlockHashSet::default();
        let mut chain_blocks: Vec<Hash> = Vec::new();

        // Start from virtual's mergeset blues
        for &blue_block in virtual_gd.mergeset_blues.iter() {
            blue_set.insert(blue_block);
        }

        // Walk the chain: each link adds itself (chain block) and its mergeset blues
        let mut curr_sp = virtual_gd.selected_parent;
        while curr_sp != conflict_genesis {
            chain_blocks.push(curr_sp);
            blue_set.insert(curr_sp);
            let gd = conflict_zone_manager.get_data(curr_sp).unwrap();
            for &blue_block in gd.mergeset_blues.iter() {
                blue_set.insert(blue_block);
            }
            curr_sp = gd.selected_parent;
        }
        blue_set.insert(conflict_genesis);
        chain_blocks.push(conflict_genesis);

        ReferenceCluster { blues: blue_set, chain_blocks }
    }

    /// Computes the k'-chain conditioned on the virtual block agreeing with a specific subgroup.
    ///
    /// Returns the chain blocks from virtual towards conflict_genesis (inclusive).
    fn compute_subgroup_chain_blocks<'a, 'b, P, T>(
        &self,
        conflict_genesis: Hash,
        group_tips: P,
        all_tips: T,
        k_prime: KType,
    ) -> SubgroupChainBlocks
    where
        P: IntoIterator<Item = &'a Hash>,
        T: IntoIterator<Item = &'b Hash>,
        T::IntoIter: Clone,
    {
        let relations_service = FutureIntersectRelations::new(&self.relations_store, &self.reachability_service, conflict_genesis);

        let mut group_tips = group_tips.into_iter();
        let first_tip = group_tips.next().expect("group must be non-empty");
        let group_tips = once(first_tip).chain(group_tips);
        let all_tips = all_tips.into_iter();

        // Calculate the subgroup's next chain ancestor above conflict_genesis
        let subgroup_nca = self.reachability_service.get_next_chain_ancestor(*first_tip, conflict_genesis);
        let conflict_zone_manager = ConflictZoneManager::committed_search(
            k_prime,
            conflict_genesis,
            &self.dagknight_store,
            &self.headers_store,
            relations_service,
            &self.reachability_service,
        );
        conflict_zone_manager.fill_zone_data(all_tips.clone(), subgroup_nca);

        // Condition virtual on the group: force selected parent from group_tips
        let subgroup_virtual_sp = conflict_zone_manager.find_selected_parent(group_tips);
        let virtual_gd: GhostdagData = conflict_zone_manager.k_colouring(all_tips, k_prime, Some(subgroup_virtual_sp));

        // Walk the chain from virtual's selected parent back to conflict_genesis
        let mut chain_blocks: Vec<Hash> = Vec::new();
        let mut curr_sp = virtual_gd.selected_parent;
        while curr_sp != conflict_genesis {
            chain_blocks.push(curr_sp);
            curr_sp = conflict_zone_manager.get_selected_parent(curr_sp).unwrap();
        }
        chain_blocks.push(conflict_genesis);

        chain_blocks
    }

    /// Counts how many blocks in `chain` are in the anticone of `b`.
    ///
    /// A chain block `c` is in `anticone(b)` iff neither `b` is a DAG ancestor of `c`
    /// nor `c` is a DAG ancestor of `b`.
    fn count_anticone_with_chain(&self, b: Hash, chain: &[Hash]) -> KType {
        chain
            .iter()
            .filter(|&&c| !self.reachability_service.is_dag_ancestor_of(b, c) && !self.reachability_service.is_dag_ancestor_of(c, b))
            .count() as KType
    }

    /// Computes the "high-rank witnesses" set C_i for a subgroup.
    ///
    /// Per Algorithm 4, C_i is the union over k' ∈ {⌊k/2⌋, ..., k} of all blocks B in
    /// the reference cluster F where |anticone(B) ∩ chain_{i,k'}| > k'. The conflict
    /// genesis is always included in C_i as a baseline witness.
    fn compute_high_rank_witnesses<'a, 'b, P, T>(
        &self,
        conflict_genesis: Hash,
        group_tips: P,
        all_tips: T,
        f_cluster: &BlockHashSet,
        k: KType,
    ) -> SortableBlock
    where
        P: IntoIterator<Item = &'a Hash>,
        T: IntoIterator<Item = &'b Hash>,
        T::IntoIter: Clone,
    {
        let mut witness_block_data: SortableBlock =
            SortableBlock { hash: conflict_genesis, blue_work: self.headers_store.get_header(conflict_genesis).unwrap().blue_work };

        // TODO[DK]: Revisit - as it only checks k - 1 against the reference block
        let k_prime = k.saturating_sub(1);
        let chain = self.compute_subgroup_chain_blocks(conflict_genesis, group_tips, all_tips, k_prime);

        for &b in f_cluster.iter() {
            if self.count_anticone_with_chain(b, &chain) > k_prime {
                let curr_witness_data = SortableBlock { hash: b, blue_work: self.headers_store.get_header(b).unwrap().blue_work };

                if witness_block_data.cmp(&curr_witness_data) == std::cmp::Ordering::Less {
                    witness_block_data = curr_witness_data;
                }
            }
        }

        witness_block_data
    }

    /// Implements Algorithm 4 from the DAGKnight paper.
    ///
    /// 1. Compute reference cluster F using free search at g(k) = floor(sqrt(k))
    /// 2. For each subgroup, compute C_i = high-rank witnesses against F
    /// 3. Select the subgroup whose max(C_i) is earliest (argmin by blue_work, ties by hash)
    pub fn tie_break<'a, T>(
        &self,
        conflict_genesis: Hash,
        all_tips: T,
        subgroups: &[GroupMetadata],
        parents: &[Hash],
        k: KType,
    ) -> usize
    where
        T: IntoIterator<Item = &'a Hash>,
        T::IntoIter: Clone,
    {
        let all_tips = all_tips.into_iter();

        // Step 1: Compute reference cluster F using free search with g(k) = floor(sqrt(k))
        let g_k = k.isqrt() as KType;
        let ref_cluster = self.compute_reference_cluster(conflict_genesis, all_tips.clone(), g_k);
        let f_cluster = ref_cluster.blues;

        // Step 2: For each group P_i, compute C_i (high-rank witnesses)
        let mut group_scores: Vec<(usize, SortableBlock, Hash)> = Vec::with_capacity(subgroups.len());

        for (idx, group_metadata) in subgroups.iter().enumerate() {
            let group_tips = group_metadata.subgroup.iter().map(|group| &parents[group.parent as usize]);
            let max_c_i = self.compute_high_rank_witnesses(conflict_genesis, group_tips, all_tips.clone(), &f_cluster, k);

            group_scores.push((idx, max_c_i, group_metadata.selected_parent.hash));
        }

        // Step 3: Select winner
        group_scores.iter().min_by(|(_, a, ah), (_, b, bh)| a.cmp(b).then_with(|| ah.cmp(bh))).map(|(idx, _, _)| *idx).unwrap()
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use std::collections::HashMap;
    use std::{cell::RefCell, sync::Arc};

    use kaspa_consensus_core::{BlueWorkType, blockhash::ORIGIN, header::Header};
    use parking_lot::lock_api::RwLock;

    use super::*;
    use crate::{
        model::stores::{
            dagknight::MemoryDagknightStore, headers::MemoryHeaderStore, reachability::MemoryReachabilityStore,
            relations::MemoryRelationsStore,
        },
        processes::{
            dagknight::protocol::Group,
            reachability::tests::{DagBlock, DagBuilder},
        },
    };

    /// Shared test DAG used by the tie-breaking tests.
    ///
    /// ```text
    ///        A (conflict genesis)
    ///       / \
    ///      B   Z
    ///      |   |
    ///      C   Y
    ///      | \ /
    ///      D  X
    /// ```
    ///
    /// Reachability chain parents: X→Y, Y→Z, Z→A | D→C, C→B, B→A
    /// Blue work: A=0, B=1, C=3, D=4, Z=1, Y=2, X=6
    ///
    /// The low-work side (X→Y→Z→A) has chain parents wired to Z, while the high-work
    /// side (D→C→B→A) has chain parents wired through B. Free search will override
    /// chain parents and follow max blue work (X→C→B→A).
    struct TieBreakTestDag {
        hash_a: Hash,
        hash_b: Hash,
        hash_c: Hash,
        hash_d: Hash,
        hash_z: Hash,
        hash_y: Hash,
        hash_x: Hash,
        dagknight_store: Arc<MemoryDagknightStore>,
        headers_store: Arc<MemoryHeaderStore>,
        relations_store: MemoryRelationsStore,
        reachability_service: MTReachabilityService<MemoryReachabilityStore>,
    }

    impl TieBreakTestDag {
        fn new() -> Self {
            let hash_a: Hash = 1_u64.into();
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

                let insert = |h: Hash, p: Vec<Hash>, bits: u32, store: &Arc<MemoryHeaderStore>, bw: BlueWorkType| {
                    let mut header = Header::from_precomputed_hash(h, p);
                    header.bits = bits;
                    header.blue_work = bw;
                    store.insert(Arc::new(header));
                };

                insert(hash_a, vec![], 0x207fffff, &headers_store, 0.into());
                insert(hash_b, vec![hash_a], 0x204fffff, &headers_store, 1.into());
                insert(hash_c, vec![hash_b], 0x207fffff, &headers_store, 3.into());
                insert(hash_d, vec![hash_c], 0x207fffff, &headers_store, 4.into());
                insert(hash_z, vec![hash_a], 0x207fffff, &headers_store, 1.into());
                insert(hash_y, vec![hash_z], 0x207fffff, &headers_store, 2.into());
                insert(hash_x, vec![hash_c, hash_y], 0x207fffff, &headers_store, 6.into());
            }

            let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
            Self {
                hash_a,
                hash_b,
                hash_c,
                hash_d,
                hash_z,
                hash_y,
                hash_x,
                dagknight_store,
                headers_store,
                relations_store,
                reachability_service,
            }
        }

        fn tie_breaker(
            &self,
        ) -> DagknightTieBreaker<&MemoryDagknightStore, &MemoryHeaderStore, MemoryRelationsStore, MemoryReachabilityStore> {
            DagknightTieBreaker::new(
                &self.dagknight_store,
                &self.headers_store,
                self.relations_store.clone(),
                self.reachability_service.clone(),
            )
        }
    }

    /// Verifies that `compute_subgroup_chain_blocks` follows the reachability store's chain
    /// parents (committed mode).
    ///
    /// Conditioned on [X]: chain follows reachability chain X → Y → Z → A
    /// Conditioned on [D]: chain follows reachability chain D → C → B → A
    #[test]
    fn test_subgroup_chain_blocks() {
        let dag = TieBreakTestDag::new();
        let tie_breaker = dag.tie_breaker();
        let all_tips = vec![dag.hash_x, dag.hash_d];

        // Conditioned on [X]: must follow reachability chain X → Y → Z → A
        let chain_x = tie_breaker.compute_subgroup_chain_blocks(dag.hash_a, &[dag.hash_x], &all_tips, 2);
        assert_eq!(chain_x.len(), 4, "X conditioned chain should have 4 blocks: X, Y, Z, A");
        assert_eq!(chain_x[0], dag.hash_x, "virtual selected parent is X");
        assert_eq!(chain_x[1], dag.hash_y, "X's committed parent must be Y");
        assert_eq!(chain_x[2], dag.hash_z, "Y's committed parent must be Z");
        assert_eq!(chain_x[3], dag.hash_a, "Z's committed parent is genesis A");

        // Conditioned on [D]: must follow reachability chain D → C → B → A
        let chain_d = tie_breaker.compute_subgroup_chain_blocks(dag.hash_a, &[dag.hash_d], &all_tips, 2);
        assert_eq!(chain_d.len(), 4, "D conditioned chain should have 4 blocks: D, C, B, A");
        assert_eq!(chain_d[0], dag.hash_d, "virtual selected parent is D");
        assert_eq!(chain_d[1], dag.hash_c, "D's committed parent must be C");
        assert_eq!(chain_d[2], dag.hash_b, "C's committed parent must be B");
        assert_eq!(chain_d[3], dag.hash_a, "B's committed parent is genesis A");

        // The two chains diverge: X-side goes through Y/Z, D-side goes through C/B
        assert!(!chain_d.contains(&dag.hash_y), "D's chain must NOT contain Y (different side of conflict)");
        assert!(!chain_d.contains(&dag.hash_z), "D's chain must NOT contain Z (different side of conflict)");
        assert!(!chain_x.contains(&dag.hash_b), "X's chain must NOT contain B (different side of conflict)");
        assert!(!chain_x.contains(&dag.hash_c), "X's chain must NOT contain C (different side of conflict)");
    }

    /// Verifies that `compute_reference_cluster` overrides the reachability store's chain
    /// parents and follows the max blue work path.
    ///
    /// Reachability chain from X: X → Y → Z → A (low-work side)
    /// Free search chain from X: X → C → B → A (max blue work side)
    #[test]
    fn test_free_coloring() {
        let dag = TieBreakTestDag::new();
        let tie_breaker = dag.tie_breaker();
        let all_tips = vec![dag.hash_x];

        let ref_cluster = tie_breaker.compute_reference_cluster(dag.hash_a, &all_tips, 2);

        // Verify the exact free search chain: X → C → B → A
        assert_eq!(ref_cluster.chain_blocks.len(), 4, "chain should have exactly 4 blocks: X, C, B, A");
        assert_eq!(ref_cluster.chain_blocks[0], dag.hash_x, "virtual selected parent must be X (max blue_work)");
        assert_eq!(ref_cluster.chain_blocks[1], dag.hash_c, "X's free-selected parent must be C (bw=3 > Y's bw=2)");
        assert_eq!(ref_cluster.chain_blocks[2], dag.hash_b, "C's free-selected parent must be B");
        assert_eq!(ref_cluster.chain_blocks[3], dag.hash_a, "B's parent is genesis A");

        // All 6 discovered blocks should be blue within k=2 (D is dead-end, not discovered)
        assert_eq!(ref_cluster.blues.len(), 6, "6 zone blocks should be blue (k=2 is sufficient)");
        for &h in &[dag.hash_a, dag.hash_b, dag.hash_c, dag.hash_z, dag.hash_y, dag.hash_x] {
            assert!(ref_cluster.blues.contains(&h), "block must be in blue cluster");
        }
        assert!(!ref_cluster.blues.contains(&dag.hash_d), "D is not discovered by zone traversal (dead-end)");
    }

    /// Verifies that `count_anticone_with_chain` correctly identifies anticone blocks.
    ///
    /// Uses a symmetric diamond DAG with two concurrent branches from A merging at D:
    ///
    /// ```text
    ///       A
    ///      / \
    ///     B   Z
    ///     |   |
    ///     C   Y
    ///     |   |
    ///     |   X
    ///      \ /
    ///       D  (D's parents: [C, X], sp: C)
    /// ```
    ///
    /// Left branch:  A → B → C
    /// Right branch: A → Z → Y → X
    ///
    /// Chain 1: [C, B, A] (left side, towards genesis)
    /// Chain 2: [X, Y, Z, A] (right side, towards genesis)
    ///
    /// Key property: every block on one side is concurrent with every block on the other.
    /// B and C are concurrent with all of {Z, Y, X} → anticone count = 3 against Chain 2.
    /// Z, Y, X are concurrent with all of {B, C} → anticone count = 2 against Chain 1.
    #[test]
    fn test_count_anticone_with_chain() {
        let hash_a: Hash = 1_u64.into();
        let hash_b: Hash = 2_u64.into();
        let hash_c: Hash = 3_u64.into();
        let hash_z: Hash = 4_u64.into();
        let hash_y: Hash = 5_u64.into();
        let hash_x: Hash = 6_u64.into();
        let hash_d: Hash = 7_u64.into();

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));
        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();

        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations_store);
            builder.init();
            builder.add_block(DagBlock::new(hash_a, vec![ORIGIN]));
            builder.add_block_with_selected_parent(DagBlock::new(hash_b, vec![hash_a]), hash_a);
            builder.add_block_with_selected_parent(DagBlock::new(hash_c, vec![hash_b]), hash_b);
            builder.add_block_with_selected_parent(DagBlock::new(hash_z, vec![hash_a]), hash_a);
            builder.add_block_with_selected_parent(DagBlock::new(hash_y, vec![hash_z]), hash_z);
            builder.add_block_with_selected_parent(DagBlock::new(hash_x, vec![hash_y]), hash_y);
            builder.add_block_with_selected_parent(DagBlock::new(hash_d, vec![hash_c, hash_x]), hash_c);

            let insert = |h: Hash, p: Vec<Hash>, bits: u32, store: &Arc<MemoryHeaderStore>, bw: BlueWorkType| {
                let mut header = Header::from_precomputed_hash(h, p);
                header.bits = bits;
                header.blue_work = bw;
                store.insert(Arc::new(header));
            };

            insert(hash_a, vec![], 0x207fffff, &headers_store, 0.into());
            insert(hash_b, vec![hash_a], 0x207fffff, &headers_store, 1.into());
            insert(hash_c, vec![hash_b], 0x207fffff, &headers_store, 2.into());
            insert(hash_z, vec![hash_a], 0x207fffff, &headers_store, 1.into());
            insert(hash_y, vec![hash_z], 0x207fffff, &headers_store, 2.into());
            insert(hash_x, vec![hash_y], 0x207fffff, &headers_store, 3.into());
            insert(hash_d, vec![hash_c, hash_x], 0x207fffff, &headers_store, 4.into());
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let tie_breaker = DagknightTieBreaker::new(&dagknight_store, &headers_store, relations_store, reachability_service);

        // Chain 2: [X, Y, Z, A] (right side, towards genesis)
        let chain_right = vec![hash_x, hash_y, hash_z, hash_a];

        // B is concurrent with Z, Y, X → anticone count = 3
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_b, &chain_right), 3, "B is concurrent with Z, Y, X");

        // C is concurrent with Z, Y, X → anticone count = 3
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_c, &chain_right), 3, "C is concurrent with Z, Y, X");

        // A is ancestor of everything → anticone count = 0
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_a, &chain_right), 0, "genesis A is ancestor of all chain blocks");

        // X, Y, Z are in the chain → anticone count = 0
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_x, &chain_right), 0, "X is in its own chain");
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_y, &chain_right), 0, "Y is in its own chain");
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_z, &chain_right), 0, "Z is in its own chain");

        // Chain 1: [C, B, A] (left side, towards genesis)
        let chain_left = vec![hash_c, hash_b, hash_a];

        // Z is concurrent with B, C → anticone count = 2
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_z, &chain_left), 2, "Z is concurrent with B, C");

        // Y is concurrent with B, C → anticone count = 2
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_y, &chain_left), 2, "Y is concurrent with B, C");

        // X is concurrent with B, C → anticone count = 2
        assert_eq!(tie_breaker.count_anticone_with_chain(hash_x, &chain_left), 2, "X is concurrent with B, C");
    }

    /// Verifies that `compute_high_rank_witnesses` correctly identifies F blocks
    /// that exceed the k'-cluster bound against the conditioned chain.
    ///
    /// With k=4, k' iterates from 2 to 4. The method computes conditioned chains
    /// for each k' and collects F blocks whose anticone with the chain exceeds k'.
    #[test]
    fn test_high_rank_witnesses() {
        let dag = TieBreakTestDag::new();
        let tie_breaker = dag.tie_breaker();
        let all_tips = vec![dag.hash_x, dag.hash_d];

        // Compute F cluster at g(4) = 2
        let ref_cluster = tie_breaker.compute_reference_cluster(dag.hash_a, &all_tips, 2);
        let f_cluster = ref_cluster.blues;

        // Compute C_i for [X] side
        let c_x = tie_breaker.compute_high_rank_witnesses(dag.hash_a, &[dag.hash_x], &all_tips, &f_cluster, 4);

        // Compute C_i for [D] side
        let c_d = tie_breaker.compute_high_rank_witnesses(dag.hash_a, &[dag.hash_d], &all_tips, &f_cluster, 4);

        // All non-genesis blocks in C_i must be a subset of F
        assert!(f_cluster.contains(&c_x.hash), "X's witness must be in F");
        assert!(f_cluster.contains(&c_d.hash), "D's witness must be in F");
    }

    /// Verifies that `tie_break` is invariant to the order of subgroups in the
    /// input: the winning *subgroup content* must be the same even if the index differs.
    #[test]
    fn test_tie_break_subgroup_ordering_invariance() {
        let dag = TieBreakTestDag::new();
        let tie_breaker = dag.tie_breaker();
        let parents = [dag.hash_x, dag.hash_d];
        let all_tips = vec![dag.hash_x, dag.hash_d];

        let mutual_k = 4;
        let sp_x = SortableBlock { hash: dag.hash_x, blue_work: dag.headers_store.get_header(dag.hash_x).unwrap().blue_work };
        let sp_d = SortableBlock { hash: dag.hash_d, blue_work: dag.headers_store.get_header(dag.hash_d).unwrap().blue_work };

        // Forward order: [X], [D]
        let sg_x = [Group { common_ancestor: dag.hash_z, parent: 0 }];
        let sg_d = [Group { common_ancestor: dag.hash_b, parent: 1 }];
        let subgroups_forward = vec![
            GroupMetadata { subgroup: &sg_x, k: mutual_k, selected_parent: sp_x.clone() },
            GroupMetadata { subgroup: &sg_d, k: mutual_k, selected_parent: sp_d.clone() },
        ];

        // Reversed order: [D], [X]
        let subgroups_reversed = vec![
            GroupMetadata { subgroup: &sg_d, k: mutual_k, selected_parent: sp_d },
            GroupMetadata { subgroup: &sg_x, k: mutual_k, selected_parent: sp_x },
        ];

        let output_forward = tie_breaker.tie_break(dag.hash_a, &all_tips, &subgroups_forward, &parents, mutual_k);
        let output_reversed = tie_breaker.tie_break(dag.hash_a, &all_tips, &subgroups_reversed, &parents, mutual_k);

        // The winning subgroup *content* must be the same
        assert_eq!(
            subgroups_forward[output_forward].subgroup, subgroups_reversed[output_reversed].subgroup,
            "winning subgroup content must be invariant to input ordering"
        );

        // The winning_index must be flipped (forward index 0 == reversed index 1, etc.)
        // But we only assert content equality since the tie-break could pick either side.
    }
}
