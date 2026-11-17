use std::cmp::Reverse;
use std::sync::Arc;

use kaspa_consensus_core::{BlockHashSet, KType};
use kaspa_core::debug;
use kaspa_hashes::Hash;
use parking_lot::RwLock;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            conflict_zone_coloring::{ColoringData, ConflictZoneColoringStore, ConflictZoneColoringStoreReader},
            headers::HeaderStoreReader,
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::{
        dagknight::{GroupMetadata, manager::ConflictZoneManager},
        ghostdag::ordering::SortableBlock,
        reachability::relations::FutureIntersectRelations,
    },
};

/// Result of free-search k-colouring reference cluster computation.
pub struct ReferenceCluster {
    /// Set of blue block hashes in the resulting colouring.
    pub blues: BlockHashSet,
    /// Chain backbone from virtual towards conflict_genesis (inclusive).
    pub chain_blocks: Vec<Hash>,
}

/// Input data for a tie-breaking call.
pub struct TieBreakContext<'a> {
    /// The latest common chain ancestor of all competing tips (conflict genesis).
    pub conflict_genesis: Hash,
    /// All tips of the DAG (parents of the virtual block).
    pub all_tips: &'a [Hash],
    /// The tied subgroups: each subgroup is a slice of tip hashes that agree on a common chain ancestor above the conflict genesis.
    pub subgroups: &'a [GroupMetadata],
    /// The minimum rank among the compared subgroups; sides may qualify one rank above it.
    pub k: KType,
}

/// Conjectured scan horizon: the length of the suffix of F the witness scan may cover, depending only on ell and growing quadratically in it.
///
/// The paper's placeholder is ell^2 ("implementation relaxation" paragraph: the required horizon, fallback rule and preservation of the growth and recovery guarantees remain to be justified); the square of the liveness window, (ell+2)^2, is used so the cap never cuts inside the window itself (pure ell^2 is degenerate for ell < 2).
///
/// TODO[DK]: tighten to the justified bound once the conjecture is proven.
fn witness_horizon(ell: KType) -> u64 {
    let side = ell as u64 + 2;
    side * side
}

/// Trait for tie-breaking logic, isolating it from the DAGKnight executor for testability.
pub trait TieBreaker {
    /// Performs tie-breaking among competing subgroups that share the same rank.
    ///
    /// Returns the index of the winning subgroup
    fn tie_break(&self, input: &TieBreakContext<'_>) -> usize;
}

/// DAGKnight tie-breaker backed by the consensus stores.
///
/// Holds only the stores needed for tie-breaking, not the full `DagknightExecutor`,
/// enabling isolated unit testing of tie-breaking logic.
pub struct DagknightTieBreaker<
    C: ConflictZoneColoringStore + ConflictZoneColoringStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> {
    pub coloring_store: Arc<C>,
    pub headers_store: Arc<O>,
    pub relations_store: Arc<RwLock<D>>,
    pub reachability_service: MTReachabilityService<R>,
}

impl<
    C: ConflictZoneColoringStore + ConflictZoneColoringStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> DagknightTieBreaker<C, O, D, R>
{
    /// A block chains to the deciding endpoint (the list head) when it sits
    /// within ell+2 positions of it, or when it is a DAG ancestor of a witness
    /// at most ell+2 positions earlier in the list.
    fn chains_to_endpoint(&self, block: Hash, i: usize, descending_f: &[Hash], is_witness: &[bool], ell: KType) -> bool {
        let window = ell as usize + 2;
        i < window
            || descending_f[i - window..i]
                .iter()
                .zip(&is_witness[i - window..i])
                .any(|(&later, &witness)| witness && self.reachability_service.is_dag_ancestor_of(block, later))
    }

    pub fn new(
        coloring_store: Arc<C>,
        headers_store: Arc<O>,
        relations_store: Arc<RwLock<D>>,
        reachability_service: MTReachabilityService<R>,
    ) -> Self {
        Self { coloring_store, headers_store, relations_store, reachability_service }
    }

    /// Computes the free-search k-colouring reference cluster.
    ///
    /// This is equivalent to `select_parent_from_k_colouring` but uses
    /// `ConflictZoneManager::with_free_search` with `free_search = true`,
    /// allowing unrestricted maximization of the k-cluster across all parents.
    ///
    /// Returns the blue set and chain backbone.
    pub fn compute_reference_cluster(&self, conflict_genesis: Hash, all_tips: &[Hash], k: KType) -> ReferenceCluster {
        let reachability_service = self.reachability_service.clone();
        let relations_store = self.relations_store.read();
        let relations_service = FutureIntersectRelations::new(relations_store.clone(), reachability_service.clone(), conflict_genesis);

        let conflict_zone_manager = ConflictZoneManager::with_free_search(
            k,
            conflict_genesis,
            self.coloring_store.clone(),
            self.headers_store.clone(),
            relations_service,
            reachability_service.clone(),
            true, // free_search = true
        );

        conflict_zone_manager.fill_zone_data(all_tips); // free_search: NCA is None

        // Run k-colouring with free search (no custom selected parent is passed,
        // so the manager freely selects from all parents).
        let virtual_gd: ColoringData = conflict_zone_manager.k_colouring(all_tips, k, None);

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

    /// Earliest start of a side's suffix witness along the descending-ordered reference, found by a forward scan from the deciding endpoint: a block is a witness when its chain anchor is the side's and it chains to the endpoint. Anchors are fetched lazily by zipping the reference with an on-demand anchor iterator, so positions past the stop are never queried (each side refetches its scanned prefix; the queries are O(1) interval lookups and sides are few).
    ///
    /// The scan stops exactly once the last witness is more than ell+1 positions behind: a causal chain cannot cross a gap wider than the window, so no later block can be a witness. Returns the earliest witness position in ascending 0-based order, or the list length when no witness exists.
    fn suffix_witness_start(&self, descending_f: &[Hash], conflict_genesis: Hash, anchor: Hash, ell: KType) -> usize {
        let n = descending_f.len();
        let window = ell as usize + 2;
        let mut is_witness: Vec<bool> = Vec::new();
        let mut last_witness: Option<usize> = None;
        let anchors =
            descending_f.iter().map(|&block| self.reachability_service.try_get_next_chain_ancestor(block, conflict_genesis).unwrap());
        for (i, (&block, chain_anchor)) in descending_f.iter().zip(anchors).enumerate() {
            if i >= window && !matches!(last_witness, Some(p) if p >= i - window) {
                break;
            }
            let witness =
                chain_anchor == Some(anchor) && (i < window || self.chains_to_endpoint(block, i, descending_f, &is_witness, ell));
            is_witness.push(witness);
            if witness {
                last_witness = Some(i);
            }
        }
        debug!("dk::tiebreak: scanned {} of {} reference positions", is_witness.len(), n);
        match last_witness {
            Some(p) => n - 1 - p,
            None => n,
        }
    }

    /// Free reference F at width g, ordered future-to-past by descending
    /// SortableBlock key. This deviates from the paper's f1..fn past-to-future
    /// listing so the suffix-witness scan runs forward from the deciding
    /// endpoint at the list head; blue work grows along ancestry, keeping
    /// parents after children.
    fn ordered_reference(&self, conflict_genesis: Hash, all_tips: &[Hash], g: KType) -> Vec<Hash> {
        // TODO[DK]: get the reference cluster already sorted (e.g. keep the coloring's
        // blues in SortableBlock order when collecting the chain walk) instead of
        // sorting O(F log F) online on every tie break
        let mut ordered_f: Vec<Hash> = self.compute_reference_cluster(conflict_genesis, all_tips, g).blues.iter().copied().collect();
        ordered_f
            .sort_by_cached_key(|block| Reverse(SortableBlock::new(*block, self.headers_store.get_header(*block).unwrap().blue_work)));
        ordered_f
    }
}

impl<
    C: ConflictZoneColoringStore + ConflictZoneColoringStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> TieBreaker for DagknightTieBreaker<C, O, D, R>
{
    /// Tiebreaking through a common reference: prefers the side whose committed path
    /// can follow the free reference F furthest back.
    ///
    /// 1. Build F as the free coloring at width g(m) = isqrt(m) over the conflict zone
    ///    and list it in a fixed topological order
    /// 2. Score each side by the earliest start of a causal chain of agreeing
    ///    reference blocks with position gaps of at most ell+2 and the last block
    ///    within ell+1 of the deciding endpoint; the scan is capped at the
    ///    conjectured horizon (see [`witness_horizon`]) and sides still live at
    ///    the cap tie into the fallback; a side without a witness scores n
    /// 3. Live-at-horizon first, then earliest start; equal scores fall back
    ///    to the smallest
    ///    selected-parent hash
    fn tie_break(&self, input: &TieBreakContext<'_>) -> usize {
        let TieBreakContext { conflict_genesis, all_tips, subgroups, k } = input;
        let m = *k;
        let g = m.isqrt() as KType;
        let ell = g.max(m / 2);
        debug!("dk::tiebreak: m = {} | g(m) = {} | ell = {} | sides = {}", m, g, ell, subgroups.len());

        // Step 1: free reference F at width g(m), future-to-past, kept only up
        // to the conjectured horizon (a suffix of F whose length depends only
        // on ell; see `witness_horizon`)
        let ordered_full = self.ordered_reference(*conflict_genesis, all_tips, g);
        let n = ordered_full.len();
        let horizon = (witness_horizon(ell) as usize).min(n);
        let truncated = n > horizon;
        let ordered_f = &ordered_full[..horizon];
        debug!("dk::tiebreak: reference F of {} blocks, horizon {}: {:?}", n, horizon, ordered_f);

        // Steps 2+3: score each side by its earliest suffix-witness start and pick the winner in one pass. Anchors are fetched lazily along each scan, so the cost tracks min(witness depth, horizon), not the reference size.
        //
        // A start within the liveness window of the cut means the witness chain may continue past the horizon: reclassify the side as live (`None`), which outranks every measured start (std `Option` ordering puts `None` first) and ties live sides into the equal-witness fallback. Dead sides keep their measured start; equal scores fall back to the smallest selected-parent hash.
        let window = ell as usize + 2;
        let winner = subgroups
            .iter()
            .map(|group| {
                let start = self.suffix_witness_start(ordered_f, *conflict_genesis, group.conflict_genesis, ell);
                let score = if truncated && start < window { None } else { Some(start) };
                debug!(
                    "dk::tiebreak: side {} (rank {}) witness score {:?} (None = live at horizon)",
                    group.conflict_genesis, group.k, score
                );
                (group, score)
            })
            .enumerate()
            .min_by(|(_, (group, score)), (_, (other, other_score))| {
                score.cmp(other_score).then_with(|| group.selected_parent.hash.cmp(&other.selected_parent.hash))
            })
            .expect("subgroups should be non-empty")
            .0;
        debug!("dk::tiebreak: winner is side {} (index {})", subgroups[winner].conflict_genesis, winner);
        winner
    }
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{BlueWorkType, blockhash::ORIGIN, header::Header};
    use std::{cell::RefCell, collections::HashMap};

    use crate::{
        model::stores::{
            conflict_zone_coloring::MemoryConflictZoneColoringStore, headers::MemoryHeaderStore,
            reachability::MemoryReachabilityStore, relations::MemoryRelationsStore,
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
        pub tie_breaker:
            DagknightTieBreaker<MemoryConflictZoneColoringStore, MemoryHeaderStore, MemoryRelationsStore, MemoryReachabilityStore>,
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
            let coloring_store = Arc::new(MemoryConflictZoneColoringStore::new(dk_map));
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
            let tie_breaker =
                DagknightTieBreaker::new(coloring_store, headers_store, Arc::new(RwLock::new(relations_store)), reachability_service);

            Self { hash_a, hash_b, hash_c, hash_d, hash_z, hash_y, hash_x, tie_breaker }
        }
    }

    /// Verifies that `compute_reference_cluster` overrides the reachability store's chain
    /// parents and follows the max blue work path.
    ///
    /// Reachability chain from X: X → Y → Z → A (low-work side)
    /// Free search chain from X: X → C → B → A (max blue work side)
    #[test]
    fn test_free_coloring() {
        let dag = TestDag::new();
        let all_tips = vec![dag.hash_x];

        let ref_cluster = dag.tie_breaker.compute_reference_cluster(dag.hash_a, &all_tips, 2);

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

    /// Verifies that `tie_break` is invariant to the order of subgroups in the
    /// input: the winning subgroup content must be the same even if the index differs.
    ///
    /// Sides: D anchored at B, X anchored at Z (see TestDag above); at mutual k=4
    /// the reference covers the zone reachable from both tips [X, D].
    #[test]
    fn test_tie_break_subgroup_ordering_invariance() {
        let dag = TestDag::new();
        let all_tips = vec![dag.hash_x, dag.hash_d];

        let mutual_k = 4;

        // Forward order: [X], [D]
        let sg_x = Arc::new(vec![dag.hash_x]);
        let sg_d = Arc::new(vec![dag.hash_d]);
        let subgroups_forward = vec![
            GroupMetadata {
                conflict_genesis: dag.hash_z,
                subgroup: sg_x,
                k: mutual_k,
                selected_parent: SortableBlock {
                    hash: dag.hash_x,
                    blue_work: dag.tie_breaker.headers_store.get_header(dag.hash_x).unwrap().blue_work,
                },
            },
            GroupMetadata {
                conflict_genesis: dag.hash_b,
                subgroup: sg_d,
                k: mutual_k,
                selected_parent: SortableBlock {
                    hash: dag.hash_d,
                    blue_work: dag.tie_breaker.headers_store.get_header(dag.hash_d).unwrap().blue_work,
                },
            },
        ];
        let input_forward =
            TieBreakContext { conflict_genesis: dag.hash_a, all_tips: &all_tips, subgroups: &subgroups_forward, k: mutual_k };

        // Reversed order: [D], [X]
        let sg_d2 = Arc::new(vec![dag.hash_d]);
        let sg_x2 = Arc::new(vec![dag.hash_x]);
        let subgroups_reversed = vec![
            GroupMetadata {
                conflict_genesis: dag.hash_b,
                subgroup: sg_d2,
                k: mutual_k,
                selected_parent: SortableBlock {
                    hash: dag.hash_d,
                    blue_work: dag.tie_breaker.headers_store.get_header(dag.hash_d).unwrap().blue_work,
                },
            },
            GroupMetadata {
                conflict_genesis: dag.hash_z,
                subgroup: sg_x2,
                k: mutual_k,
                selected_parent: SortableBlock {
                    hash: dag.hash_x,
                    blue_work: dag.tie_breaker.headers_store.get_header(dag.hash_x).unwrap().blue_work,
                },
            },
        ];
        let input_reversed =
            TieBreakContext { conflict_genesis: dag.hash_a, all_tips: &all_tips, subgroups: &subgroups_reversed, k: mutual_k };

        let output_forward = dag.tie_breaker.tie_break(&input_forward);
        let output_reversed = dag.tie_breaker.tie_break(&input_reversed);

        // The winning subgroup *content* must be the same
        assert_eq!(
            subgroups_forward[output_forward].subgroup, subgroups_reversed[output_reversed].subgroup,
            "winning subgroup content must be invariant to input ordering"
        );

        // The winning_index must be flipped (forward index 0 == reversed index 1, etc.)
        // But we only assert content equality since the tie-break could pick either side.
    }

    type TestTieBreaker =
        DagknightTieBreaker<MemoryConflictZoneColoringStore, MemoryHeaderStore, MemoryRelationsStore, MemoryReachabilityStore>;

    /// Builds linear chain sides above a shared genesis with per-block (blue_score,
    /// blue_work), blue_work controlling the reference order; returns the tie breaker,
    /// the genesis and the per-side chains (anchor first)
    fn chain_sides_dag(sides: &[&[(u64, u64)]]) -> (TestTieBreaker, Hash, Vec<Vec<Hash>>) {
        let genesis: Hash = 1_u64.into();
        let dk_map = RefCell::new(HashMap::new());
        let coloring_store = Arc::new(MemoryConflictZoneColoringStore::new(dk_map));
        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();
        let mut chains = Vec::with_capacity(sides.len());
        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations_store);
            builder.init();
            builder.add_block(DagBlock::new(genesis, vec![ORIGIN]));
            let mut header = Header::from_precomputed_hash(genesis, vec![]);
            header.bits = 0x207fffff;
            headers_store.insert(Arc::new(header));

            let mut next_id = 2_u64;
            for side in sides {
                let mut chain = Vec::with_capacity(side.len());
                let mut parent = genesis;
                for &(blue_score, blue_work) in side.iter() {
                    let hash: Hash = next_id.into();
                    next_id += 1;
                    builder.add_block_with_selected_parent(DagBlock::new(hash, vec![parent]), parent);
                    let mut header = Header::from_precomputed_hash(hash, vec![parent]);
                    header.bits = 0x207fffff;
                    header.blue_score = blue_score;
                    header.blue_work = blue_work.into();
                    headers_store.insert(Arc::new(header));
                    chain.push(hash);
                    parent = hash;
                }
                chains.push(chain);
            }
        }
        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let tie_breaker =
            DagknightTieBreaker::new(coloring_store, headers_store, Arc::new(RwLock::new(relations_store)), reachability_service);
        (tie_breaker, genesis, chains)
    }

    fn tips(chains: &[Vec<Hash>]) -> Vec<Hash> {
        chains.iter().map(|c| *c.last().unwrap()).collect()
    }

    fn group_metadata(anchor: Hash, tip: Hash, k: KType, blue_work: u64) -> GroupMetadata {
        GroupMetadata {
            conflict_genesis: anchor,
            subgroup: Arc::new(vec![tip]),
            k,
            selected_parent: SortableBlock::new(tip, blue_work.into()),
        }
    }

    fn short(hash: &Hash) -> u64 {
        hash.to_le_u64()[3]
    }

    /// Walks the tiebreaking rule stage by stage, printing the reference, per-side
    /// agreement and witness starts. Run with `-- --nocapture` to follow how
    /// witnesses are generated.
    ///
    /// Two chain sides interleaved by blue work:
    ///
    /// ```text
    ///         genesis (0)
    ///         /      \
    ///     B1 (1)     Z1 (2)
    ///       |          |
    ///     B2 (3)     Z2 (4)
    /// ```
    ///
    /// Side 0 anchors at B1, side 1 at Z1. At m=4 (g=2, ell=2, window 4) F holds
    /// all five blocks, future-to-past: [Z2, B2, Z1, B1, genesis]. Side 0 agrees
    /// at positions {1, 3} and side 1 at {0, 2} (0-based, future-to-past).
    /// Ascending past-to-future, also 0-based: genesis=0, B1=1, Z1=2, B2=3,
    /// Z2=4. Both chains are causal with gaps of 2, so side 0 witnesses from
    /// B1 at 1, one further back than side 1's Z1 at 2.
    #[test]
    fn test_tiebreak_walkthrough() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(1, 1), (3, 3)], &[(2, 2), (4, 4)]]);
        let all_tips = tips(&chains);
        let m: KType = 4;
        let g = m.isqrt() as KType;
        let ell = g.max(m / 2);

        // The reference F at width g(m), ordered and anchored like the production path
        let ordered = tie_breaker.ordered_reference(genesis, &all_tips, g);
        let anchors = |block: Hash| tie_breaker.reachability_service.try_get_next_chain_ancestor(block, genesis).unwrap();

        println!("m = {m} | g(m) = {g} | ell = {ell} | window (ell+2) = {}", ell + 2);
        println!("reference F, future-to-past (genesis last): {:?}", ordered.iter().map(short).collect::<Vec<_>>());

        let mut starts = vec![];
        for (side_idx, chain) in chains.iter().enumerate() {
            let anchor = chain[0];
            let agreement: Vec<usize> =
                ordered.iter().enumerate().filter(|&(_, &block)| anchors(block) == Some(anchor)).map(|(i, _)| i).collect();
            let start = tie_breaker.suffix_witness_start(&ordered, genesis, anchor, ell);
            println!(
                "side {} (anchor {}): agrees at future-to-past positions {agreement:?}, witness score {start:?}",
                side_idx,
                short(&anchor)
            );
            starts.push(start);
        }

        let subgroups = vec![group_metadata(chains[0][0], chains[0][1], m, 1), group_metadata(chains[1][0], chains[1][1], m, 1)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: m };
        let winner = tie_breaker.tie_break(&input);
        println!("winner: side {winner}");
        assert_eq!(starts, vec![1, 2], "the earlier-interleaved side witnesses further back");
        assert_eq!(winner, 0);
    }

    /// The side whose agreeing reference blocks start earliest wins, regardless of
    /// input order.
    ///
    /// ```text
    ///         genesis (0)
    ///         /      \
    ///     B1 (1)     Z1 (2)
    ///       |          |
    ///     B2 (3)     Z2 (4)
    /// ```
    ///
    /// Side 0 anchors at B1, side 1 at Z1. At m=4 (window 4) F is all five blocks,
    /// future-to-past [Z2, B2, Z1, B1, genesis]: side 0 agrees at {1, 3} and side 1
    /// at {0, 2} (0-based, future-to-past); ascending 0-based puts B1 at 1 and
    /// Z1 at 2, so side 0's B1 start beats side 1's Z1.
    #[test]
    fn test_tie_break_earliest_start_wins() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(1, 1), (3, 3)], &[(2, 2), (4, 4)]]);
        let all_tips = tips(&chains);
        // Order: [genesis, B1, Z1, B2, Z2]; at m=4 the window ell+2 = 4 covers both chains
        let subgroups = vec![group_metadata(chains[0][0], chains[0][1], 4, 1), group_metadata(chains[1][0], chains[1][1], 4, 1)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: 4 };
        assert_eq!(tie_breaker.tie_break(&input), 0, "witness start at 1 beats 2");

        let subgroups = vec![group_metadata(chains[1][0], chains[1][1], 4, 1), group_metadata(chains[0][0], chains[0][1], 4, 1)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: 4 };
        assert_eq!(tie_breaker.tie_break(&input), 1, "index flips with input order, same winner");
    }

    /// A gap between agreeing blocks beyond ell+2 breaks the witness chain.
    ///
    /// ```text
    ///         genesis (0)
    ///         /      \
    ///     B1 (2)     Z1 (3)
    ///       |          |
    ///     B2 (5)     Z2 (4)
    /// ```
    ///
    /// Side 0 anchors at B1, side 1 at Z1. F by work, future-to-past:
    /// [B2, Z2, Z1, B1, genesis]; at ell=0 the window is 2. The B side agrees at
    /// B2@0 and B1@3, three positions apart, so only B2 witnesses (start 4); the
    /// Z side agrees at Z2@1 and Z1@2 with gap 1 and witnesses from Z1 (start 2).
    #[test]
    fn test_witness_start_respects_gap_limit() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(2, 2), (5, 5)], &[(3, 3), (4, 4)]]);
        // Future-to-past reference by blue work: [B2, Z2, Z1, B1, genesis]; at ell=0 the window is 2
        let ordered = vec![chains[0][1], chains[1][1], chains[1][0], chains[0][0], genesis];
        assert_eq!(
            tie_breaker.suffix_witness_start(&ordered, genesis, chains[0][0], 0),
            4,
            "B1 at position 1 is 3 positions from B2, beyond the window"
        );
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, genesis, chains[1][0], 0), 2);
    }

    /// A side whose agreeing blocks never reach the endpoint within the window has
    /// no witness and scores the endpoint position n.
    ///
    /// ```text
    ///              genesis (0)
    ///            /    |      \
    ///        B (2)   Z (3)   Y1 (4) -> Y2 (5) -> Y3 (6) -> Y4 (7)
    /// ```
    ///
    /// F by work, future-to-past: [Y4, Y3, Y2, Y1, Z, B, genesis]; at ell=0 the
    /// window is 2. The B side agrees only at B@5, too far from any other agreeing
    /// block, so it scores the endpoint 7; the Y side chains with gap 1 and
    /// witnesses from Y1 (start 3).
    #[test]
    fn test_witness_without_witness_returns_endpoint_position() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(2, 2)], &[(3, 3)], &[(4, 4), (5, 5), (6, 6), (7, 7)]]);
        // Future-to-past reference by blue work: [Y4, Y3, Y2, Y1, Z, B, genesis]
        let ordered = vec![chains[2][3], chains[2][2], chains[2][1], chains[2][0], chains[1][0], chains[0][0], genesis];
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, genesis, chains[0][0], 0), 7);
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, genesis, chains[2][0], 0), 3);
    }

    /// Concurrent agreeing blocks within the window do not extend the witness; the
    /// same topology with a causal link does.
    ///
    /// ```text
    ///  fork:  A <- B <- {C1, Z}, C2 <- B    (C1 and C2 concurrent)
    ///  chain: A <- B <- Z, B <- C1 <- C2    (C1 causally below C2)
    /// ```
    ///
    /// Works: A=0, B=1, C1=2, Z=3, C2=4; the side anchors at B and agrees at C2,
    /// C1 and B. F is [C2, Z, C1, B, A] in both variants, and at ell=0 the window
    /// is 2. In the fork C1@2 is concurrent with C2@0 and cannot chain into it, so
    /// only C2 witnesses (start 4); in the chain C1 links to C2 and B to C1,
    /// reaching start 1.
    #[test]
    fn test_witness_requires_causality() {
        let build = |c2_parents: Vec<Hash>| {
            let hash_a: Hash = 1_u64.into();
            let hash_b: Hash = 2_u64.into();
            let hash_c1: Hash = 3_u64.into();
            let hash_z: Hash = 4_u64.into();
            let hash_c2: Hash = 5_u64.into();

            let dk_map = RefCell::new(HashMap::new());
            let coloring_store = Arc::new(MemoryConflictZoneColoringStore::new(dk_map));
            let headers_store = Arc::new(MemoryHeaderStore::new());
            let mut reachability = MemoryReachabilityStore::new();
            let mut relations_store = MemoryRelationsStore::new();
            {
                let mut builder = DagBuilder::new(&mut reachability, &mut relations_store);
                builder.init();
                builder.add_block(DagBlock::new(hash_a, vec![ORIGIN]));
                builder.add_block_with_selected_parent(DagBlock::new(hash_b, vec![hash_a]), hash_a);
                builder.add_block_with_selected_parent(DagBlock::new(hash_c1, vec![hash_b]), hash_b);
                builder.add_block_with_selected_parent(DagBlock::new(hash_z, vec![hash_a]), hash_a);
                builder.add_block_with_selected_parent(DagBlock::new(hash_c2, c2_parents.clone()), c2_parents[0]);

                let insert = |h: Hash, p: Vec<Hash>, blue_work: u64| {
                    let mut header = Header::from_precomputed_hash(h, p);
                    header.bits = 0x207fffff;
                    header.blue_work = blue_work.into();
                    headers_store.insert(Arc::new(header));
                };
                insert(hash_a, vec![], 0);
                insert(hash_b, vec![hash_a], 1);
                insert(hash_c1, vec![hash_b], 2);
                insert(hash_z, vec![hash_a], 3);
                insert(hash_c2, c2_parents, 4);
            }
            let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
            let tie_breaker =
                DagknightTieBreaker::new(coloring_store, headers_store, Arc::new(RwLock::new(relations_store)), reachability_service);
            (tie_breaker, hash_a, hash_b)
        };

        // Future-to-past reference by blue work: [C2, Z, C1, B, A]; at ell=0 the window is 2
        let ordered = vec![5_u64.into(), 4_u64.into(), 3_u64.into(), 2_u64.into(), 1_u64.into()];

        let (tie_breaker, hash_a, hash_b) = build(vec![2_u64.into()]);
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, hash_a, hash_b, 0), 4, "concurrent C1 cannot chain into C2");

        let (tie_breaker, hash_a, hash_b) = build(vec![3_u64.into()]);
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, hash_a, hash_b, 0), 1, "the causal chain B < C1 < C2 witnesses from B");
    }

    /// The scan stops exactly once the last witness is more than ell+1 positions
    /// behind: blocks beyond the gap are never evaluated (the lazy anchor
    /// iterator is simply never pulled further), and the score equals the
    /// full-list result.
    ///
    /// ```text
    ///              genesis
    ///            /    |      \
    ///       B1-B2    Z     T1-T2-T3-T4
    /// ```
    ///
    /// F by work, future-to-past: [B1(100), T1(90), T2(80), T3(70), T4(60),
    /// B2(50), Z(1)]. At ell=0 the window is 2: B1@0 is a witness, the T run
    /// opens a gap, so B2@5 could not chain to the endpoint even if scanned:
    /// the scan breaks at i=3 and the start is the full-list answer n-1 = 6.
    #[test]
    fn test_early_stop_never_fetches_past_the_gap() {
        let (tie_breaker, genesis, chains) =
            chain_sides_dag(&[&[(2, 100), (3, 50)], &[(4, 1)], &[(5, 90), (6, 80), (7, 70), (8, 60)]]);
        let ordered = vec![chains[0][0], chains[2][0], chains[2][1], chains[2][2], chains[2][3], chains[0][1], chains[1][0]];
        assert_eq!(
            tie_breaker.suffix_witness_start(&ordered, genesis, chains[0][0], 0),
            6,
            "exact stop: full-list answer without scanning past the gap"
        );
    }

    /// The horizon truncation drives the live/dead classification in `tie_break`: a start within the liveness window of the cut (ascending start < window, in truncated-list coordinates) marks the side live, a dead side keeps its measured start, and scores are only compared within the same truncated coordinates.
    ///
    /// Reference [B2, Z2, Z1, B1, genesis] (the gap-limit DAG) at ell=0 (window 2), truncated to 3 = [B2, Z2, Z1]: the Z side's deepest witness Z1@2 gives start 0 < 2 (live), the B side's B2@0 gives start 2 (dead); untruncated scores stay 4 and 2.
    #[test]
    fn test_horizon_cap_marks_live_sides() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(2, 2), (5, 5)], &[(3, 3), (4, 4)]]);
        let ordered = vec![chains[0][1], chains[1][1], chains[1][0], chains[0][0], genesis];
        let (window, horizon) = (2_usize, 3_usize);
        let truncated = ordered.len() > horizon;

        // tie_break's classification over the truncated reference
        let score = |anchor| {
            let start = tie_breaker.suffix_witness_start(&ordered[..horizon], genesis, anchor, 0);
            if truncated && start < window { None } else { Some(start) }
        };
        assert_eq!(score(chains[1][0]), None, "Z's deepest witness sits at the cut: live at horizon");
        assert_eq!(score(chains[0][0]), Some(2), "B's chain is dead at the cut: exact truncated start");

        // Untruncated scores are unchanged by the cap
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, genesis, chains[0][0], 0), 4);
        assert_eq!(tie_breaker.suffix_witness_start(&ordered, genesis, chains[1][0], 0), 2);

        // Live-at-horizon (None) outranks every measured start via std Option ordering
        assert!(None < Some(0));
    }

    /// `witness_horizon` is quadratic in the liveness window and never cuts inside it: (ell+2)^2.
    #[test]
    fn test_witness_horizon_is_quadratic() {
        assert_eq!(witness_horizon(0), 4);
        assert_eq!(witness_horizon(1), 9);
        assert_eq!(witness_horizon(2), 16);
        assert_eq!(witness_horizon(10), 144);
    }

    /// End-to-end tie break under a binding horizon: at m=0 (ell=0, g=0, horizon 4) the free reference is the max-work chain, here B's five blocks plus genesis (6 > 4, truncated). B's witness chain is live at the cut (start 0 < window 2) so B scores live-at-horizon and beats Z's no-witness score, matching the untruncated semantics where B's witnesses run deepest.
    #[test]
    fn test_tie_break_with_binding_horizon() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(1, 10), (2, 20), (3, 30), (4, 40), (5, 50)], &[(6, 1)]]);
        let all_tips = tips(&chains);
        let subgroups = vec![group_metadata(chains[0][0], chains[0][4], 0, 50), group_metadata(chains[1][0], chains[1][0], 0, 1)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: 0 };
        assert_eq!(tie_breaker.tie_break(&input), 0, "the live-at-horizon side wins");

        let subgroups = vec![group_metadata(chains[1][0], chains[1][0], 0, 1), group_metadata(chains[0][0], chains[0][4], 0, 50)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: 0 };
        assert_eq!(tie_breaker.tie_break(&input), 1, "index flips with input order, same winner");
    }

    /// With both sides witness-less the smallest selected-parent hash decides.
    ///
    /// ```text
    ///              genesis (0)
    ///            /    |      \
    ///        B (1)   Z (1)   Y1..Y4 (work 9 each, one chain)
    /// ```
    ///
    /// At m=0 the free coloring at g=0 is a single chain following max work, so F
    /// holds only the Y chain: the compared sides B and Z have no agreeing
    /// reference blocks at all, both score the endpoint, and B's smaller tip hash
    /// wins in either input order.
    #[test]
    fn test_tie_break_fallback_on_equal_starts() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(2, 1)], &[(3, 1)], &[(4, 9), (5, 9), (6, 9), (7, 9)]]);
        let all_tips = tips(&chains);
        let subgroups = vec![group_metadata(chains[0][0], chains[0][0], 0, 1), group_metadata(chains[1][0], chains[1][0], 0, 1)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: 0 };
        assert_eq!(tie_breaker.tie_break(&input), 0, "the smaller tip hash wins");

        let subgroups = vec![group_metadata(chains[1][0], chains[1][0], 0, 1), group_metadata(chains[0][0], chains[0][0], 0, 1)];
        let input = TieBreakContext { conflict_genesis: genesis, all_tips: &all_tips, subgroups: &subgroups, k: 0 };
        assert_eq!(tie_breaker.tie_break(&input), 1, "the smaller tip hash wins regardless of input order");
    }
}
