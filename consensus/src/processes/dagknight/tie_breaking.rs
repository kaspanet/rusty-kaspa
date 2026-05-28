use std::sync::Arc;

use kaspa_consensus_core::{BlockHashSet, KType};
use kaspa_hashes::Hash;
use parking_lot::RwLock;

use crate::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            dagknight::{DagknightStore, DagknightStoreReader},
            ghostdag::GhostdagData,
            headers::HeaderStoreReader,
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::{dagknight::manager::ConflictZoneManager, reachability::relations::FutureIntersectRelations},
};

/// Input data for a tie-breaking call.
pub struct TieBreakInput<'a> {
    /// The latest common chain ancestor of all competing tips (conflict genesis).
    pub conflict_genesis: Hash,
    /// All tips of the DAG (parents of the virtual block).
    pub all_tips: &'a [Hash],
    /// The tied subgroups — each subgroup is a slice of tip hashes that agree on a common chain ancestor above the conflict genesis.
    pub subgroups: &'a [&'a [Hash]],
    /// The mutual rank `k` shared by all tied subgroups.
    pub k: KType,
}

/// Output of a tie-breaking call: the index of the winning subgroup and the subgroup hashes.
pub struct TieBreakOutput {
    /// Index of the winning subgroup within the input `subgroups` slice.
    pub winning_index: usize,
    /// The hashes belonging to the winning subgroup.
    pub winning_subgroup: Vec<Hash>,
}

/// Trait for tie-breaking logic, isolating it from the DAGKnight executor for testability.
pub trait TieBreaker {
    /// Performs tie-breaking among competing subgroups that share the same rank.
    ///
    /// Returns the winning subgroup. Implementations must be deterministic: the same
    /// input must always produce the same output.
    fn tie_break(&self, input: &TieBreakInput<'_>) -> TieBreakOutput;
}

/// DAGKnight tie-breaker backed by the consensus stores.
///
/// Holds only the stores needed for tie-breaking, not the full `DagknightExecutor`,
/// enabling isolated unit testing of tie-breaking logic.
pub struct DagknightTieBreaker<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> {
    pub dagknight_store: Arc<C>,
    pub headers_store: Arc<O>,
    pub relations_store: Arc<RwLock<D>>,
    pub reachability_service: MTReachabilityService<R>,
}

impl<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> DagknightTieBreaker<C, O, D, R>
{
    pub fn new(
        dagknight_store: Arc<C>,
        headers_store: Arc<O>,
        relations_store: Arc<RwLock<D>>,
        reachability_service: MTReachabilityService<R>,
    ) -> Self {
        Self { dagknight_store, headers_store, relations_store, reachability_service }
    }

    /// Computes the free-search k-colouring reference cluster.
    ///
    /// This is equivalent to `select_parent_from_k_colouring` but uses
    /// `ConflictZoneManager::with_free_search` with `free_search = true`,
    /// allowing unrestricted maximization of the k-cluster across all parents.
    ///
    /// Returns the set of blue block hashes in the resulting colouring.
    pub fn compute_free_coloring(&self, conflict_genesis: Hash, all_tips: &[Hash], k: KType) -> (BlockHashSet, Vec<Hash>) {
        let reachability_service = self.reachability_service.clone();
        let relations_store = self.relations_store.read();
        let relations_service = FutureIntersectRelations::new(relations_store.clone(), reachability_service.clone(), conflict_genesis);

        let conflict_zone_manager = ConflictZoneManager::with_free_search(
            k,
            conflict_genesis,
            self.dagknight_store.clone(),
            self.headers_store.clone(),
            relations_service,
            reachability_service.clone(),
            true, // free_search = true
        );

        conflict_zone_manager.fill_zone_data(all_tips);

        // Run k-colouring with free search — no custom selected parent is passed,
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

        (blue_set, chain_blocks)
    }

    /// Computes the k'-chain conditioned on the virtual block agreeing with a specific subgroup.
    ///
    /// This is a **committed** (non-free-search) k-colouring: the virtual block's selected
    /// parent is forced to be the max blue-work block from `group_tips`, committing the
    /// colouring to that group's side of the conflict. During zone population, each block
    /// only considers "agreeing" parents (those sharing a chain ancestor above conflict_genesis).
    ///
    /// Returns the chain backbone from virtual towards conflict_genesis (inclusive).
    pub fn compute_conditioned_chain(
        &self,
        conflict_genesis: Hash,
        group_tips: &[Hash],
        all_tips: &[Hash],
        k_prime: KType,
    ) -> Vec<Hash> {
        let reachability_service = self.reachability_service.clone();
        let relations_store = self.relations_store.read();
        let relations_service = FutureIntersectRelations::new(relations_store.clone(), reachability_service.clone(), conflict_genesis);

        // Committed (non-free-search) manager
        let conflict_zone_manager = ConflictZoneManager::with_free_search(
            k_prime,
            conflict_genesis,
            self.dagknight_store.clone(),
            self.headers_store.clone(),
            relations_service,
            reachability_service.clone(),
            false, // free_search = false (committed)
        );

        conflict_zone_manager.fill_zone_data(all_tips);

        // Condition virtual on the group: force selected parent from group_tips
        let subgroup_virtual_sp = conflict_zone_manager.find_selected_parent(group_tips.iter().copied());
        let virtual_gd: GhostdagData = conflict_zone_manager.k_colouring(all_tips, k_prime, Some(subgroup_virtual_sp));

        // Walk the chain from virtual's selected parent back to conflict_genesis
        let mut chain_blocks: Vec<Hash> = Vec::new();
        let mut curr_sp = virtual_gd.selected_parent;
        while curr_sp != conflict_genesis {
            chain_blocks.push(curr_sp);
            let gd = conflict_zone_manager.get_data(curr_sp).unwrap();
            curr_sp = gd.selected_parent;
        }
        chain_blocks.push(conflict_genesis);

        chain_blocks
    }
}

impl<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> TieBreaker for DagknightTieBreaker<C, O, D, R>
{
    /// Implements Algorithm 4 from the DAGKnight paper.
    ///
    /// Currently implements Step 1 (computing the free reference cluster F).
    /// Steps 2-3 (computing C_i for each group and selecting the winner) are deferred.
    ///
    /// TODO[DK]: Implement full Algorithm 4 tie-breaking with C_i computation
    fn tie_break(&self, input: &TieBreakInput<'_>) -> TieBreakOutput {
        let TieBreakInput { conflict_genesis, all_tips, subgroups, k } = input;

        // Step 1: Compute reference cluster F using free search with g(k) = floor(sqrt(k))
        let g_k = k.isqrt() as KType;
        let (_f_cluster, _chain_blocks) = self.compute_free_coloring(*conflict_genesis, all_tips, g_k);

        // TODO[DK]: Step 2 - For each group P_i, compute chains and C_i sets
        // TODO[DK]: Step 3 - Select winner with argmin of max(C_i), break ties by hash

        // Placeholder: fall back to deterministic selection by subgroup hash comparison
        let winner_idx = (0..subgroups.len()).min_by_key(|&i| subgroups[i][0]).expect("subgroups must be non-empty");

        TieBreakOutput { winning_index: winner_idx, winning_subgroup: subgroups[winner_idx].to_vec() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{BlueWorkType, blockhash::ORIGIN, header::Header};
    use std::{cell::RefCell, collections::HashMap};

    use crate::{
        model::stores::{
            dagknight::MemoryDagknightStore, headers::MemoryHeaderStore, reachability::MemoryReachabilityStore,
            relations::MemoryRelationsStore,
        },
        processes::reachability::tests::{DagBlock, DagBuilder},
    };

    /// Shared test DAG used by all tie_breaking tests.
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
    struct TestDag {
        pub hash_a: Hash,
        pub hash_b: Hash,
        pub hash_c: Hash,
        pub hash_d: Hash,
        pub hash_z: Hash,
        pub hash_y: Hash,
        pub hash_x: Hash,
        pub tie_breaker: DagknightTieBreaker<
            MemoryDagknightStore,
            MemoryHeaderStore,
            MemoryRelationsStore,
            MemoryReachabilityStore,
        >,
    }

    impl TestDag {
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

                let insert =
                    |h: Hash, p: Vec<Hash>, bits: u32, store: &Arc<MemoryHeaderStore>, bw: BlueWorkType| {
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
            let tie_breaker = DagknightTieBreaker::new(
                dagknight_store,
                headers_store,
                Arc::new(RwLock::new(relations_store)),
                reachability_service,
            );

            Self { hash_a, hash_b, hash_c, hash_d, hash_z, hash_y, hash_x, tie_breaker }
        }
    }

    /// Verifies that `compute_conditioned_chain` follows the reachability store's chain
    /// parents (committed mode).
    ///
    /// Conditioned on [X]: chain follows reachability chain X → Y → Z → A
    /// Conditioned on [D]: chain follows reachability chain D → C → B → A
    #[test]
    fn test_conditioned_chain() {
        let dag = TestDag::new();
        let all_tips = vec![dag.hash_x, dag.hash_d];

        // Conditioned on [X]: must follow reachability chain X → Y → Z → A
        let chain_x = dag.tie_breaker.compute_conditioned_chain(dag.hash_a, &[dag.hash_x], &all_tips, 2);
        assert_eq!(chain_x.len(), 4, "X conditioned chain should have 4 blocks: X, Y, Z, A");
        assert_eq!(chain_x[0], dag.hash_x, "virtual selected parent is X");
        assert_eq!(chain_x[1], dag.hash_y, "X's committed parent must be Y");
        assert_eq!(chain_x[2], dag.hash_z, "Y's committed parent must be Z");
        assert_eq!(chain_x[3], dag.hash_a, "Z's committed parent is genesis A");

        // Conditioned on [D]: must follow reachability chain D → C → B → A
        let chain_d = dag.tie_breaker.compute_conditioned_chain(dag.hash_a, &[dag.hash_d], &all_tips, 2);
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

    /// Verifies that `compute_free_coloring` overrides the reachability store's chain
    /// parents and follows the max blue work path.
    ///
    /// Reachability chain from X: X → Y → Z → A (low-work side)
    /// Free search chain from X: X → C → B → A (max blue work side)
    #[test]
    fn test_free_coloring() {
        let dag = TestDag::new();
        let all_tips = vec![dag.hash_x];

        let (blue_cluster, chain_blocks) =
            dag.tie_breaker.compute_free_coloring(dag.hash_a, &all_tips, 2);

        // Verify the exact free search chain: X → C → B → A
        assert_eq!(chain_blocks.len(), 4, "chain should have exactly 4 blocks: X, C, B, A");
        assert_eq!(chain_blocks[0], dag.hash_x, "virtual selected parent must be X (max blue_work)");
        assert_eq!(chain_blocks[1], dag.hash_c, "X's free-selected parent must be C (bw=3 > Y's bw=2)");
        assert_eq!(chain_blocks[2], dag.hash_b, "C's free-selected parent must be B");
        assert_eq!(chain_blocks[3], dag.hash_a, "B's parent is genesis A");

        // All 6 discovered blocks should be blue within k=2 (D is dead-end, not discovered)
        assert_eq!(blue_cluster.len(), 6, "6 zone blocks should be blue (k=2 is sufficient)");
        for &h in &[dag.hash_a, dag.hash_b, dag.hash_c, dag.hash_z, dag.hash_y, dag.hash_x] {
            assert!(blue_cluster.contains(&h), "block must be in blue cluster");
        }
        assert!(!blue_cluster.contains(&dag.hash_d), "D is not discovered by zone traversal (dead-end)");
    }
}
