use std::collections::VecDeque;
use std::sync::Arc;

use kaspa_consensus_core::KType;
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
        dagknight::{GroupMetadata, manager::ConflictZoneManager, reference_iterator::ReferenceIterator},
        reachability::relations::FutureIntersectRelations,
    },
};

/// Input data for a tie-breaking call.
pub struct TieBreakContext<'a> {
    /// The latest common chain ancestor of all competing tips (conflict genesis).
    pub conflict_genesis: Hash,
    /// All tips of the DAG (parents of the virtual block).
    pub all_tips: &'a [Hash],
    /// The tied subgroups — each subgroup is a slice of tip hashes that agree on a common chain ancestor above the conflict genesis.
    pub subgroups: &'a [GroupMetadata],
    /// The minimum rank among the compared subgroups; sides may qualify one rank above it.
    pub k: KType,
}

/// Liveness window for the witness scan: ell+2 reference positions, the widest gap a
/// witness chain can bridge.
fn witness_window(ell: KType) -> usize {
    ell as usize + 2
}

/// Conjectured horizon capping the witness scan: (ell+2)^2, quadratic in the liveness window and never cutting inside the window itself.
///
/// TODO[DK]: tighten to the justified bound once the conjecture is proven.
fn witness_horizon(ell: KType) -> u64 {
    let side = witness_window(ell) as u64;
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
    /// A block chains to the deciding endpoint (the scan head) when it is a DAG
    /// ancestor of a same-side witness among the previous ell+2 reference
    /// positions; the buffer holds exactly those positions, newest first (the
    /// scan head at the front), each with the side it witnessed for.
    fn chains_to_endpoint(&self, block: Hash, subgroup_nca: Hash, recent: &VecDeque<(Hash, Option<Hash>)>) -> bool {
        recent.iter().any(|&(later, witness_nca)| {
            witness_nca == Some(subgroup_nca) && self.reachability_service.is_dag_ancestor_of(block, later)
        })
    }

    pub fn new(
        coloring_store: Arc<C>,
        headers_store: Arc<O>,
        relations_store: Arc<RwLock<D>>,
        reachability_service: MTReachabilityService<R>,
    ) -> Self {
        Self { coloring_store, headers_store, relations_store, reachability_service }
    }

    /// Free-search k-colouring over the conflict zone: fills the zone below
    /// `all_tips` and colours the deciding block above them.
    ///
    /// This is equivalent to `select_parent_from_k_colouring` but uses
    /// `ConflictZoneManager::with_free_search` with `free_search = true`,
    /// allowing unrestricted maximization of the k-cluster across all parents.
    ///
    /// Returns the filled zone manager and the deciding block's colouring data.
    pub fn free_coloring(
        &self,
        conflict_genesis: Hash,
        all_tips: &[Hash],
        k: KType,
    ) -> (ConflictZoneManager<C, O, D, R>, ColoringData) {
        let relations_store = self.relations_store.read().clone();
        let relations_service = FutureIntersectRelations::new(relations_store, self.reachability_service.clone(), conflict_genesis);

        let zone = ConflictZoneManager::with_free_search(
            k,
            conflict_genesis,
            self.coloring_store.clone(),
            self.headers_store.clone(),
            relations_service,
            self.reachability_service.clone(),
            true, // free_search = true
        );

        zone.fill_zone_data(all_tips);

        let virtual_cd = zone.k_colouring(all_tips, k, None);
        (zone, virtual_cd)
    }

    /// Returns the earliest start of each side's suffix-witness chain along the descending-ordered reference prefix of length `n`, one start per subgroup NCA in `subgroup_ncas` (ascending 0-based positions, or `n` for a side with no witness), in a single pass shared by all sides. A block is a witness when its block NCA is one of `subgroup_ncas` and it chains to the endpoint through witnesses of its own side. The scan stops once every side's last witness is over ell+1 positions behind (a causal chain cannot cross a wider gap), so blocks past the stop are never queried.
    fn suffix_witness_starts(
        &self,
        mut descending_f: impl Iterator<Item = Hash>,
        n: usize,
        conflict_genesis: Hash,
        subgroup_ncas: &[Hash],
        ell: KType,
    ) -> Vec<usize> {
        let window = witness_window(ell);
        let mut recent: VecDeque<(Hash, Option<Hash>)> = VecDeque::with_capacity(window);
        // Per-side start: n while unwitnessed, n-1-i once position i witnesses
        let mut starts = vec![n; subgroup_ncas.len()];
        let mut scanned = 0usize;
        let mut i = 0;
        // Keep scanning while some side can resume from position i: its deepest
        // witness sits within the liveness window (an unwitnessed side, start n,
        // stays resumable only through the initial window)
        while let Some(block) = descending_f.next()
            && starts.iter().any(|&start| start + i < n + window)
        {
            scanned += 1;
            let block_nca = self.reachability_service.try_get_next_chain_ancestor(block, conflict_genesis).unwrap();
            // Positions inside the initial window witness unconditionally; later
            // ones must chain to the endpoint through same-side witnesses
            let mut witness_nca = None;
            if let Some(side) = subgroup_ncas.iter().position(|&nca| block_nca == Some(nca))
                && (i < window || self.chains_to_endpoint(block, subgroup_ncas[side], &recent))
            {
                witness_nca = block_nca;
                starts[side] = n - 1 - i;
            }
            if recent.len() == window {
                recent.pop_back();
            }
            recent.push_front((block, witness_nca));
            i += 1;
        }
        debug!("dk::tiebreak: scanned {} of {} reference positions across {} sides", scanned, n, subgroup_ncas.len());
        starts
    }
}

impl<
    C: ConflictZoneColoringStore + ConflictZoneColoringStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> TieBreaker for DagknightTieBreaker<C, O, D, R>
{
    /// Tiebreaking through a common reference: prefers the side whose committed path can follow the free reference F furthest back.
    ///
    /// Sides are scored by the earliest start of their suffix-witness chain along F (see [`suffix_witness_starts`]), all scanned in one pass over F capped at the conjectured horizon. A side whose chain is still live at the cap outranks every measured start; equal scores fall back to the smallest selected-parent hash.
    fn tie_break(&self, input: &TieBreakContext<'_>) -> usize {
        let TieBreakContext { conflict_genesis, all_tips, subgroups, k } = input;
        let m = *k;
        let g_m = m.isqrt() as KType;
        let ell = g_m.max(m / 2);
        debug!("dk::tiebreak: m = {} | g(m) = {} | ell = {} | sides = {}", m, g_m, ell, subgroups.len());

        // Free reference F at width g(m), future-to-past, truncated at the horizon
        let (zone, virtual_cd) = self.free_coloring(*conflict_genesis, all_tips, g_m);
        let n = virtual_cd.blue_score as usize;
        let horizon = (witness_horizon(ell) as usize).min(n);
        let truncated = n > horizon;
        debug!("dk::tiebreak: reference F of {} blocks, horizon {}", n, horizon);

        // Score each side by its earliest suffix-witness start, all sides sharing
        // one pass over F. A start within the liveness window of the cut may
        // continue past the horizon: classify the side live (-1), outranking every
        // measured start
        let window = witness_window(ell);
        let subgroup_ncas: Vec<Hash> = subgroups.iter().map(|group| group.nca).collect();
        let starts = self.suffix_witness_starts(
            ReferenceIterator::new(&virtual_cd, *conflict_genesis, &zone, &*self.headers_store).take(horizon),
            horizon,
            *conflict_genesis,
            &subgroup_ncas,
            ell,
        );
        let winner = subgroups
            .iter()
            .zip(starts.iter())
            .map(|(group, &start)| {
                let score = if truncated && start < window { -1 } else { start as isize };
                debug!("dk::tiebreak: side {} (rank {}) witness score {} (-1 = live at horizon)", group.nca, group.k, score);
                (group, score)
            })
            .enumerate()
            .min_by(|(_, (group, score)), (_, (other, other_score))| {
                score.cmp(other_score).then_with(|| group.selected_parent.hash.cmp(&other.selected_parent.hash))
            })
            .expect("subgroups should be non-empty")
            .0;
        debug!("dk::tiebreak: winner is side {} (index {})", subgroups[winner].nca, winner);
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
        processes::{
            ghostdag::ordering::SortableBlock,
            reachability::tests::{DagBlock, DagBuilder},
        },
        test_helpers::insert_header,
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

    /// Verifies that `free_coloring` overrides the reachability store's chain
    /// parents and follows the max blue work path.
    ///
    /// Reachability chain from X: X → Y → Z → A (low-work side)
    /// Free search chain from X: X → C → B → A (max blue work side)
    #[test]
    fn test_free_coloring() {
        let dag = TestDag::new();
        let all_tips = vec![dag.hash_x];

        let (zone, virtual_cd) = dag.tie_breaker.free_coloring(dag.hash_a, &all_tips, 2);

        // Verify the exact free search chain: X → C → B → A
        assert_eq!(virtual_cd.selected_parent, dag.hash_x, "virtual selected parent must be X (max blue_work)");
        assert_eq!(
            zone.get_data(dag.hash_x).unwrap().selected_parent,
            dag.hash_c,
            "X's free-selected parent must be C (bw=3 > Y's bw=2)"
        );
        assert_eq!(zone.get_data(dag.hash_c).unwrap().selected_parent, dag.hash_b, "C's free-selected parent must be B");
        assert_eq!(zone.get_data(dag.hash_b).unwrap().selected_parent, dag.hash_a, "B's parent is genesis A");

        // All 6 discovered blocks are blue within k=2 (D is dead-end, not discovered)
        let blues: Vec<Hash> = ReferenceIterator::new(&virtual_cd, dag.hash_a, &zone, &*dag.tie_breaker.headers_store).collect();
        assert_eq!(
            blues,
            vec![dag.hash_x, dag.hash_c, dag.hash_y, dag.hash_z, dag.hash_b, dag.hash_a],
            "6 zone blocks are blue, future-to-past"
        );
        assert!(!blues.contains(&dag.hash_d), "D is not discovered by zone traversal (dead-end)");
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
                nca: dag.hash_z,
                subgroup: sg_x,
                k: mutual_k,
                selected_parent: SortableBlock {
                    hash: dag.hash_x,
                    blue_work: dag.tie_breaker.headers_store.get_header(dag.hash_x).unwrap().blue_work,
                },
            },
            GroupMetadata {
                nca: dag.hash_b,
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
                nca: dag.hash_b,
                subgroup: sg_d2,
                k: mutual_k,
                selected_parent: SortableBlock {
                    hash: dag.hash_d,
                    blue_work: dag.tie_breaker.headers_store.get_header(dag.hash_d).unwrap().blue_work,
                },
            },
            GroupMetadata {
                nca: dag.hash_z,
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
        GroupMetadata { nca: anchor, subgroup: Arc::new(vec![tip]), k, selected_parent: SortableBlock::new(tip, blue_work.into()) }
    }

    fn short(hash: &Hash) -> u64 {
        hash.to_le_u64()[3]
    }

    /// Walks the tiebreaking rule stage by stage, printing the reference, per-side
    /// agreement and witness starts. Run with `-- --nocapture` to follow along.
    ///
    /// Two chain sides interleaved by blue work (B1 < Z1 < B2 < Z2 in work), so
    /// F is [Z2, B2, Z1, B1, genesis] future-to-past and side 0's witness chain
    /// starts one position further back than side 1's.
    #[test]
    fn test_tiebreak_walkthrough() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(1, 1), (3, 3)], &[(2, 2), (4, 4)]]);
        let all_tips = tips(&chains);
        let m: KType = 4;
        let g = m.isqrt() as KType;
        let ell = g.max(m / 2);

        // The reference F at width g(m), ordered and anchored like the production path
        let (zone, virtual_cd) = tie_breaker.free_coloring(genesis, &all_tips, g);
        let ordered: Vec<Hash> = ReferenceIterator::new(&virtual_cd, genesis, &zone, &*tie_breaker.headers_store).collect();
        let anchors = |block: Hash| tie_breaker.reachability_service.try_get_next_chain_ancestor(block, genesis).unwrap();

        println!("m = {m} | g(m) = {g} | ell = {ell} | window (ell+2) = {}", ell + 2);
        println!("reference F, future-to-past (genesis last): {:?}", ordered.iter().map(short).collect::<Vec<_>>());

        let mut starts = vec![];
        let side_anchors: Vec<Hash> = chains.iter().map(|chain| chain[0]).collect();
        let all_starts = tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), genesis, &side_anchors, ell);
        for (side_idx, chain) in chains.iter().enumerate() {
            let anchor = chain[0];
            let agreement: Vec<usize> =
                ordered.iter().enumerate().filter(|&(_, &block)| anchors(block) == Some(anchor)).map(|(i, _)| i).collect();
            let start = all_starts[side_idx];
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
        let ordered = [chains[0][1], chains[1][1], chains[1][0], chains[0][0], genesis];
        let starts =
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), genesis, &[chains[0][0], chains[1][0]], 0);
        assert_eq!(starts[0], 4, "B1 at position 1 is 3 positions from B2, beyond the window");
        assert_eq!(starts[1], 2);
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
        let ordered = [chains[2][3], chains[2][2], chains[2][1], chains[2][0], chains[1][0], chains[0][0], genesis];
        let starts =
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), genesis, &[chains[0][0], chains[2][0]], 0);
        assert_eq!(starts[0], 7);
        assert_eq!(starts[1], 3);
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
        let ordered = [5_u64.into(), 4_u64.into(), 3_u64.into(), 2_u64.into(), 1_u64.into()];

        let (tie_breaker, hash_a, hash_b) = build(vec![2_u64.into()]);
        assert_eq!(
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), hash_a, std::slice::from_ref(&hash_b), 0)[0],
            4,
            "concurrent C1 cannot chain into C2"
        );

        let (tie_breaker, hash_a, hash_b) = build(vec![3_u64.into()]);
        assert_eq!(
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), hash_a, std::slice::from_ref(&hash_b), 0)[0],
            1,
            "the causal chain B < C1 < C2 witnesses from B"
        );
    }

    /// The lookback window spans exactly ell+2 positions: an agreeing ancestor exactly
    /// ell+2 positions later still witnesses, one position further is forgotten.
    ///
    /// ```text
    ///         genesis
    ///        /     \
    ///       s       t
    ///      / \      |
    ///     S2  X     T2
    ///      \ /
    ///       S3        (parents S2, X; chain parent S2)
    /// ```
    ///
    /// Reference, future-to-past: [S3, T2, S2, t, X]. At ell = 0 the window is 2.
    #[test]
    fn test_witness_lookback_spans_exactly_the_window() {
        let (genesis, s, t, x, s2, t2, s3): (Hash, Hash, Hash, Hash, Hash, Hash, Hash) =
            (1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into(), 5_u64.into(), 6_u64.into(), 7_u64.into());

        let dk_map = RefCell::new(HashMap::new());
        let coloring_store = Arc::new(MemoryConflictZoneColoringStore::new(dk_map));
        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();
        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations_store);
            builder.init();
            builder.add_block(DagBlock::new(genesis, vec![ORIGIN]));
            builder.add_block_with_selected_parent(DagBlock::new(s, vec![genesis]), genesis);
            builder.add_block_with_selected_parent(DagBlock::new(t, vec![genesis]), genesis);
            builder.add_block_with_selected_parent(DagBlock::new(x, vec![s]), s);
            builder.add_block_with_selected_parent(DagBlock::new(s2, vec![s]), s);
            builder.add_block_with_selected_parent(DagBlock::new(t2, vec![t]), t);
            builder.add_block_with_selected_parent(DagBlock::new(s3, vec![s2, x]), s2);

            let insert = |h: Hash, p: Vec<Hash>, blue_work: u64| insert_header(&headers_store, h, p, blue_work.into());
            insert(genesis, vec![], 0);
            insert(s, vec![genesis], 2);
            insert(t, vec![genesis], 5);
            insert(x, vec![s], 3);
            insert(s2, vec![s], 7);
            insert(t2, vec![t], 9);
            insert(s3, vec![s2, x], 11);
        }
        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let tie_breaker =
            DagknightTieBreaker::new(coloring_store, headers_store, Arc::new(RwLock::new(relations_store)), reachability_service);

        let ordered = [s3, t2, s2, t, x];
        assert_eq!(
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), genesis, std::slice::from_ref(&s), 0)[0],
            2
        );
    }

    /// A witness chains to the endpoint only through witnesses of its own side:
    /// a side-s block with a side-t witness descendant in its window does not
    /// extend side s.
    ///
    /// ```text
    ///         genesis
    ///        /      \
    ///       s        t
    ///       |        |
    ///       m        t2
    ///        \      /
    ///          l (parents m, t2; chain parent t2)
    ///          |
    ///          f
    /// ```
    ///
    /// Reference, future-to-past: [f, l, m, t2, t, s, genesis]; at ell = 0 the
    /// window is 2. f and l witness for t; m sits on side s with l in its
    /// window, yet m does not witness for s (start n = 7), while t witnesses
    /// down to t2 (start 2).
    #[test]
    fn test_witness_links_only_within_a_side() {
        let (genesis, s, t, m, t2, l, f): (Hash, Hash, Hash, Hash, Hash, Hash, Hash) =
            (1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into(), 5_u64.into(), 6_u64.into(), 7_u64.into());

        let dk_map = RefCell::new(HashMap::new());
        let coloring_store = Arc::new(MemoryConflictZoneColoringStore::new(dk_map));
        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();
        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations_store);
            builder.init();
            builder.add_block(DagBlock::new(genesis, vec![ORIGIN]));
            builder.add_block_with_selected_parent(DagBlock::new(s, vec![genesis]), genesis);
            builder.add_block_with_selected_parent(DagBlock::new(t, vec![genesis]), genesis);
            builder.add_block_with_selected_parent(DagBlock::new(m, vec![s]), s);
            builder.add_block_with_selected_parent(DagBlock::new(t2, vec![t]), t);
            builder.add_block_with_selected_parent(DagBlock::new(l, vec![m, t2]), t2);
            builder.add_block_with_selected_parent(DagBlock::new(f, vec![l]), l);

            let insert = |h: Hash, p: Vec<Hash>, blue_work: u64| insert_header(&headers_store, h, p, blue_work.into());
            insert(genesis, vec![], 0);
            insert(s, vec![genesis], 1);
            insert(t, vec![genesis], 2);
            insert(m, vec![s], 5);
            insert(t2, vec![t], 4);
            insert(l, vec![m, t2], 9);
            insert(f, vec![l], 10);
        }
        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
        let tie_breaker =
            DagknightTieBreaker::new(coloring_store, headers_store, Arc::new(RwLock::new(relations_store)), reachability_service);

        let ordered = [f, l, m, t2, t, s, genesis];
        assert_eq!(
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), genesis, &[s, t], 0),
            vec![7, 2],
            "m chains to the endpoint only through side-s witnesses"
        );
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
        let ordered = [chains[0][0], chains[2][0], chains[2][1], chains[2][2], chains[2][3], chains[0][1], chains[1][0]];
        let mut pulls = 0usize;
        let scanned = ordered.iter().copied().inspect(|_| pulls += 1);
        assert_eq!(
            tie_breaker.suffix_witness_starts(scanned, ordered.len(), genesis, std::slice::from_ref(&chains[0][0]), 0)[0],
            6,
            "exact stop: full-list answer without scanning past the gap"
        );
        assert_eq!(pulls, 4, "the scan pulls the breaking position and nothing past the gap");
    }

    /// All sides are scored in one pass over the shared reference: interleaved
    /// live sides cost a single full walk, not one walk per side.
    ///
    /// Reference [B2, Z2, Z1, B1, genesis] at ell = 0 (window 2): both sides
    /// stay live to the end, so the scan covers all 5 positions once and
    /// returns the per-side starts [4, 2].
    #[test]
    fn test_all_sides_share_one_reference_pass() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(2, 2), (5, 5)], &[(3, 3), (4, 4)]]);
        let ordered = [chains[0][1], chains[1][1], chains[1][0], chains[0][0], genesis];
        let mut pulls = 0usize;
        let scanned = ordered.iter().copied().inspect(|_| pulls += 1);
        let starts = tie_breaker.suffix_witness_starts(scanned, ordered.len(), genesis, &[chains[0][0], chains[1][0]], 0);
        assert_eq!(starts, vec![4, 2], "same starts as separate per-side scans");
        assert_eq!(pulls, 5, "one reference pass shared by all sides");
    }

    /// The horizon truncation drives the live/dead classification in `tie_break`: a start within the liveness window of the cut marks the side live, a dead side keeps its measured start.
    ///
    /// Reference [B2, Z2, Z1, B1, genesis] at ell=0 (window 2), truncated to 3: the Z side's deepest witness Z1@2 gives start 0 < 2 (live), the B side's B2@0 gives start 2 (dead); untruncated scores stay 4 and 2.
    #[test]
    fn test_horizon_cap_marks_live_sides() {
        let (tie_breaker, genesis, chains) = chain_sides_dag(&[&[(2, 2), (5, 5)], &[(3, 3), (4, 4)]]);
        let ordered = [chains[0][1], chains[1][1], chains[1][0], chains[0][0], genesis];
        let (window, horizon) = (2_usize, 3_usize);
        let truncated = ordered.len() > horizon;

        // tie_break's classification over the truncated reference
        let score = |anchor| {
            let start = tie_breaker.suffix_witness_starts(
                ordered[..horizon].iter().copied(),
                horizon,
                genesis,
                std::slice::from_ref(&anchor),
                0,
            )[0];
            if truncated && start < window { -1 } else { start as isize }
        };
        assert_eq!(score(chains[1][0]), -1, "Z's deepest witness sits at the cut: live at horizon");
        assert_eq!(score(chains[0][0]), 2, "B's chain is dead at the cut: exact truncated start");

        // Untruncated scores are unchanged by the cap
        let starts =
            tie_breaker.suffix_witness_starts(ordered.iter().copied(), ordered.len(), genesis, &[chains[0][0], chains[1][0]], 0);
        assert_eq!(starts[0], 4);
        assert_eq!(starts[1], 2);
    }

    /// `witness_horizon` is quadratic in the liveness window and never cuts inside it: (ell+2)^2.
    #[test]
    fn test_witness_horizon_is_quadratic() {
        assert_eq!(witness_horizon(0), 4);
        assert_eq!(witness_horizon(1), 9);
        assert_eq!(witness_horizon(2), 16);
        assert_eq!(witness_horizon(10), 144);
    }

    /// End-to-end tie break under a binding horizon: at m=0 the free reference is B's max-work chain (6 blocks > horizon 4, truncated); B's witnesses are live at the cut and beat Z's no-witness score.
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
