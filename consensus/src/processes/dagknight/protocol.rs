use crate::processes::dagknight::tie_breaking::{ReferenceCluster, SubgroupChainBlocks};
#[cfg(feature = "baseline-debugging")]
use crate::processes::dagknight::umc_baseline::BaselineUmcVoter;
use crate::processes::dagknight::umc_voting::{UmcVoter, UmcVotingContext};
use itertools::Itertools;
use kaspa_consensus_core::{BlockHashSet, KType};
use kaspa_core::debug;
use kaspa_hashes::Hash;
use smallvec::SmallVec;
use std::iter::once;
use std::ops::Deref;
use std::sync::Arc;

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
        dagknight::{
            DagknightCounters,
            manager::ConflictZoneManager,
            rank_search::RankSearcher,
            umc_cascade::SegmentTreeUmcVoter,
            umc_cascade_persistence::{UmcCascadeStore, UmcCascadeStoreReader},
            umc_voting::CascadeResult,
        },
        ghostdag::ordering::SortableBlock,
        reachability::relations::FutureIntersectRelations,
    },
};

/*
    Task 0:
        Hierarchic conflict resolution

        input: set of parents P (|P| >= 1)
        output:  a selected parent p \in P
        pseudo:

        while |P| > 1:
            g = find the latest common chain ancestor of P // the genesis of the conflict
            split P into subgroups {P_1, ..., P_n} such that blocks within each subgroup agree about the chain ancestor above g // each such subgroup is "united" re the conflict zone induced by g
            run some deterministic black box protocol F to choose a winner group P_i // to start with, xor all hashes in each subgroup and rank the results by lexicographic hash order
            P = P_i
        p = P[0]
        return p

    Task 1:
        Goal: a more sophisticated F
        Possible idea: fix k, run GD over subdag = future(g) \cup past(P), select P_i which contains the GD selected parent from P
        Main challenge: adapt the GD protocol to run on such a subdag (defined by future and past constrains). We did something like this in the pruning proof by abstracting the relations store

    Task 2:
        Vanilla DK
        Implement F with basic DK logic, i.e., searching the k space
        TBD

    ------------

    Notation: the version of k-coloring where the set of parents you can inherit a blueset for is restricted to to those
              agreeing with you, should be named DK-committed coloring (megachain = DK-chain)

    There are 3 usages of GD coloring out of selected chain:
        1. coinbase rewards
        2. blue score (mainly for blue depth but also for client confirmation counting)
        3. blue work (mainly for topological sorting and related usages)

    Q. how do keep all these with DK?

    A.
        For 1. 2. the answer is to have an incremental coloring with a fixed k over the main DK chain (name: global incremental/committed coloring )
        For 3. it seems like we need a global free coloring (probably same fixed k)

    ------------

    Possible next steps:
        1. move code to correct place
        2. moving to DK storage objects
        3. switch GD/k-coloring to committed coloring
*/

#[derive(Clone)]
pub struct DagknightData {
    pub selected_parent: Hash,               // The selected parent for this call
    pub conflict_ordered_parents: Vec<Hash>, // The rest of the parents, ordered by conflict hierarchy (parents from latest/topmost conflicts first)
}

/// A parent (index into the tips slice) paired with the chain ancestor it follows
/// above the current conflict genesis; ordering by (common_ancestor, parent) keeps
/// each agreement group contiguous within a sorted grouping
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Group {
    common_ancestor: Hash,
    parent: u16,
}

/// Ranking metadata for an agreement group: the group as a contiguous slice of the
/// sorted agreement grouping, together with the winning k and the selected parent
/// found for it. The subgroup is uniform in `common_ancestor` (the left part of
/// each member), so the group's conflict genesis is read off the members instead
/// of being duplicated here.
struct GroupMetadata<'a> {
    subgroup: &'a [Group],
    k: KType,
    selected_parent: SortableBlock,
}

/// A struct encapsulating the logic and algorithms of the DAGKNIGHT protocol
#[derive(Clone)]
pub struct DagknightExecutor<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    E: UmcCascadeStoreReader + Clone + 'static,
    R: ReachabilityStoreReader + Clone,
> {
    pub genesis_hash: Hash,
    pub dagknight_store: Arc<C>,
    pub headers_store: Arc<O>,
    pub relations_store: D,
    pub umc_persistence_store: Arc<E>,
    pub reachability_service: MTReachabilityService<R>,
    pub counters: Arc<DagknightCounters>,
}

impl<
    C: DagknightStore + DagknightStoreReader,
    O: HeaderStoreReader + 'static,
    D: RelationsStoreReader + Clone,
    E: UmcCascadeStore + Clone,
    R: ReachabilityStoreReader + Clone,
> DagknightExecutor<C, O, D, E, R>
{
    /// Resolves the selected parent and conflict-ordered parents for the given block parents
    pub fn dagknight(&self, parents: &[Hash]) -> DagknightData {
        let data = self.dagknight_indices(parents);
        DagknightData {
            selected_parent: parents[data.selected_parent as usize],
            conflict_ordered_parents: data
                .reverse_conflict_ordered_parents
                .iter()
                .rev()
                .map(|&parent| parents[parent as usize])
                .collect(),
        }
    }

    // TODO[DK]: accept slice with unique values so we don't need to verify it again
    fn dagknight_indices(&self, parents: &[Hash]) -> DagknightDataIndices {
        /*
            input: a set of block parents
            output: the selected parent + incremental metadata

            Algo scheme:
                Run DK from the bottom up per conflict, for each conflict search through k and find the minimal
                committed k-cluster which confirms to UMC cascade voting with parameter d=sqrt(k)

            High-level tasks/challenges:
                1. Incremental k-colouring -- known from GD
                2. Iterating through conflicts -- requires finding the common chain-ancestor which
                   is a simple operation, though it might require optimizing with an indexed chain
                   (and using logarithmic step searches)
                3. Representatives (alternatively: gray blocks)
                4. Tie-breaking rule
                5. Cascade voting -- requires most thought for making incremental
        */
        assert!(parents.len() <= u16::MAX as usize);
        // Duplicate parents always land in one group and can never be split apart
        let mut curr_subgroup: SmallVec<[u16; 20]> = (0..parents.len() as u16).unique_by(|&parent| parents[parent as usize]).collect();
        let mut conflict_ordered_parents: SmallVec<[u16; 20]> = SmallVec::with_capacity(parents.len());
        loop {
            curr_subgroup = match core::mem::take(&mut curr_subgroup).deref() {
                [sp] => {
                    // Returned in natural push order (bottom-most conflicts first) so consumers can
                    // reverse-iterate to walk parents from latest/topmost conflicts first
                    debug!("dk::sp: {} | reverse_conflict_ordered_parents: {:?}", sp, conflict_ordered_parents);
                    return DagknightDataIndices { selected_parent: *sp, reverse_conflict_ordered_parents: conflict_ordered_parents };
                }
                curr_subgroup => {
                    // g = find the LCCA of the current subgroup -- the genesis of this conflict level
                    let conflict_genesis = self.common_chain_ancestor(parents, curr_subgroup);
                    debug!("conflict_genesis: {:#}", conflict_genesis);

                    // Split the subgroup by the chain ancestor each parent follows above `conflict_genesis`;
                    // parents within each group agree about the conflict zone induced by `conflict_genesis`
                    let mut agreement_grouping: SmallVec<[Group; 10]> = curr_subgroup
                        .iter()
                        .map(|&parent| Group {
                            common_ancestor: self
                                .reachability_service
                                .get_next_chain_ancestor(parents[parent as usize], conflict_genesis),
                            parent,
                        })
                        .collect();
                    agreement_grouping.sort_unstable();

                    if agreement_grouping.iter().map(|group| &group.common_ancestor).all_equal() {
                        // There is exactly one group, we don't rank; the loop head re-derives the
                        // conflict genesis from this same subgroup to skip to the next level
                        agreement_grouping.into_iter().map(|group| group.parent).collect()
                    } else {
                        // Pick a "winner" among these subgroups
                        let best_groups = self.rank(conflict_genesis, parents, &agreement_grouping);

                        // Multiple best groups (same winning k) are resolved by a tie-breaking rule
                        let winner: &GroupMetadata = if best_groups.len() > 1 {
                            &best_groups[self.tie_breaking(conflict_genesis, parents, &agreement_grouping, &best_groups)]
                        } else {
                            best_groups.first().expect("rank should return at least one best group")
                        };

                        // The winners continue as the next subgroup; the losers join the conflict-ordered parents
                        let winning_ancestor = winner.subgroup[0].common_ancestor;
                        agreement_grouping
                            .iter()
                            .filter(|group| group.common_ancestor != winning_ancestor)
                            .for_each(|group| conflict_ordered_parents.push(group.parent));
                        winner.subgroup.iter().map(|group| group.parent).collect()
                    }
                }
            }
        }
    }

    /// Follows the Calculate-Rank algorithm in the DK paper: returns the best
    /// agreement groups (all surviving at the winning k) with their metadata,
    /// each subgroup being a contiguous slice into the sorted agreement grouping
    fn rank<'a>(&self, conflict_genesis: Hash, parents: &[Hash], agreement_grouping: &'a [Group]) -> SmallVec<[GroupMetadata<'a>; 4]> {
        // Groups failing a k evaluation are dropped from further consideration
        // (passing the UMC cascade is monotone in k)
        let mut survivors: SmallVec<[&'a [Group]; 10]> =
            agreement_grouping.chunk_by(|a, b| a.common_ancestor == b.common_ancestor).collect();
        let evaluate = |k: KType| -> Option<SmallVec<[GroupMetadata<'a>; 4]>> {
            let (next_survivors, best_groups): (SmallVec<_>, SmallVec<_>) = survivors
                .iter()
                .filter_map(|&subgroup| {
                    self.select_parent_from_k_colouring(
                        conflict_genesis,
                        subgroup.iter().map(|group| &parents[group.parent as usize]),
                        agreement_grouping.iter().map(|group| &parents[group.parent as usize]),
                        k,
                    )
                    .map(|selected_parent| (subgroup, GroupMetadata { subgroup, k, selected_parent }))
                })
                .unzip();

            if next_survivors.is_empty() {
                None
            } else {
                survivors = next_survivors;
                Some(best_groups)
            }
        };

        RankSearcher::search(evaluate).map(|result| result.result).unwrap_or_default()
    }

    /// Tie-breaking rule in case of multiple winning subgroups with the same rank value
    fn tie_breaking(
        &self,
        conflict_genesis: Hash,
        parents: &[Hash],
        agreement_grouping: &[Group],
        subgroups: &[GroupMetadata],
    ) -> usize {
        debug!("Winning groups had rank k = {}", subgroups[0].k);
        let mutual_k = subgroups[0].k;

        DagknightTieBreaker {
            dagknight_store: &*self.dagknight_store,
            headers_store: &*self.headers_store,
            relations_store: self.relations_store.clone(),
            reachability_service: self.reachability_service.clone(),
        }
        .tie_break(
            conflict_genesis,
            agreement_grouping.iter().map(|group| &parents[group.parent as usize]),
            subgroups,
            parents,
            mutual_k,
        )
    }

    /// Baseline UMC cascade voting: naive reference impl of paper Algorithm 6 (work-weighted),
    /// used to cross-check `SegmentTreeUmcVoter` (see the comparison in
    /// `select_parent_from_k_colouring`).
    #[cfg(feature = "baseline-debugging")]
    fn baseline_umc_cascade_voting(
        &self,
        conflict_genesis: Hash,
        first_subgroup_member: &Hash,
        virtual_gd: GhostdagData,
        k: KType,
        conflict_zone_manager: &ConflictZoneManager<&C, &O, &D, &MTReachabilityService<R>>,
    ) -> CascadeResult {
        let voter = BaselineUmcVoter::new(self.headers_store.clone(), self.reachability_service.clone());
        let ctx = UmcVotingContext {
            conflict_genesis,
            subgroup_member: first_subgroup_member,
            virtual_gd: &virtual_gd,
            k,
            coloring_reader: conflict_zone_manager,
        };
        voter.vote(&ctx)
    }

    /// UMC Cascade Voting using chain-based segment tree
    fn umc_cascade_voting(
        &self,
        conflict_genesis: Hash,
        first_subgroup_member: &Hash,
        virtual_gd: GhostdagData,
        k: KType,
        conflict_zone_manager: &ConflictZoneManager<&C, &O, &D, &MTReachabilityService<R>>,
    ) -> CascadeResult {
        let voter = SegmentTreeUmcVoter::new(
            self.headers_store.clone(),
            self.umc_persistence_store.clone(),
            self.reachability_service.clone(),
        );
        let ctx = UmcVotingContext {
            conflict_genesis,
            subgroup_member: first_subgroup_member,
            virtual_gd: &virtual_gd,
            k,
            coloring_reader: conflict_zone_manager,
        };
        voter.vote(&ctx)
    }

    /// Applies a coloring to the conflict zone, and determines if the
    /// coloring represents a majority over "g" only (as opposed to full UMC)
    /// TODO[DK]: Implement full UMC cascade voting after coloring
    fn select_parent_from_k_colouring<'a, P, T>(
        &self,
        conflict_genesis: Hash,
        subgroup: P,
        all_tips: T,
        k_to_check: KType,
    ) -> Option<SortableBlock>
    where
        P: IntoIterator<Item = &'a Hash>,
        P::IntoIter: Clone,
        T: IntoIterator<Item = &'a Hash>,
        T::IntoIter: Clone,
    {
        let subgroup = subgroup.into_iter();
        // UMC voting only reads the subgroup's first member, so keep it alongside the iterator
        let first_subgroup_member = *subgroup.clone().next().expect("subgroup must be non-empty");
        let all_tips = all_tips.into_iter();

        let relations_service = FutureIntersectRelations::new(&self.relations_store, &self.reachability_service, conflict_genesis);

        let conflict_zone_manager = ConflictZoneManager::committed_search(
            k_to_check,
            conflict_genesis,
            self.dagknight_store.as_ref(),
            self.headers_store.as_ref(),
            relations_service,
            &self.reachability_service,
        );

        // Calculate the subgroup's next chain ancestor above conflict_genesis
        let subgroup_nca = self.reachability_service.get_next_chain_ancestor(first_subgroup_member, conflict_genesis);
        conflict_zone_manager.fill_zone_data(subgroup.clone(), Some(subgroup_nca));

        // selected a parent in this subgroup => Conditioned upon virtual agreeing with this subgroup
        let subgroup_virtual_sp = conflict_zone_manager.find_selected_parent(subgroup);
        let virtual_gd = conflict_zone_manager.k_colouring(all_tips, k_to_check, Some(subgroup_virtual_sp));

        let cascade_result =
            self.umc_cascade_voting(conflict_genesis, &first_subgroup_member, virtual_gd.clone(), k_to_check, &conflict_zone_manager);

        #[cfg(feature = "baseline-debugging")]
        {
            // Compare baseline (per-blue recursive) against cascade (global virtual score)
            // These use different acceptance criteria and are not expected to always agree.
            // The baseline is Algorithm 6 from the paper; the cascade is the optimized implementation.
            let baseline_result = self.baseline_umc_cascade_voting(
                conflict_genesis,
                &first_subgroup_member,
                virtual_gd.clone(),
                k_to_check,
                &conflict_zone_manager,
            );

            if baseline_result.virtual_score != cascade_result.virtual_score {
                if baseline_result.accepted != cascade_result.accepted {
                    self.counters.record_baseline_disagreement(baseline_result.accepted, cascade_result.accepted);
                }

                panic!(
                    "BASELINE vs CASCADE SCORE DISAGREEMENT: k={}, conflict_genesis={:?}, baseline_score={}, \
                     cascade_score={}, baseline_accepted={}, cascade_accepted={}, flips={}, voting_blocks={}",
                    k_to_check,
                    conflict_genesis,
                    baseline_result.virtual_score,
                    cascade_result.virtual_score,
                    baseline_result.accepted,
                    cascade_result.accepted,
                    baseline_result.flips,
                    baseline_result.voting_blocks
                );
            }
        }

        self.counters.record_cascade_stats(cascade_result.flips, cascade_result.voting_blocks);
        self.counters.record_checkpoint_stats(
            cascade_result.from_checkpoint,
            cascade_result.estimated_effort_saved,
            cascade_result.estimated_effort_total,
        );

        if cascade_result.accepted {
            Some(SortableBlock {
                hash: subgroup_virtual_sp,
                blue_work: self.headers_store.get_header(subgroup_virtual_sp).unwrap().blue_work,
            })
        } else {
            None
        }
    }

    /// Finds the latest common chain ancestor of the given subgroup (indices into `parents`),
    /// serving as the genesis of the conflict level the caller is about to resolve
    fn common_chain_ancestor(&self, parents: &[Hash], subgroup: &[u16]) -> Hash {
        // TODO: DK
        /*
           Notes:
               - ignore/exclude/make-lose parents not agreeing on the pruning point as a chain block
               - optimize for shortest path
               - optimize with index
        */

        let start = parents[subgroup[0] as usize];

        if start == self.genesis_hash {
            return self.genesis_hash;
        }

        for cb in self.reachability_service.default_backward_chain_iterator(start).skip(1) {
            if self.reachability_service.is_chain_ancestor_of_all(cb, subgroup.iter().skip(1).map(|&parent| &parents[parent as usize]))
            {
                return cb;
            }
        }

        unreachable!()
    }
}

/// DAGKnight tie-breaker implementing Algorithm 4 of the paper. Holds only the
/// stores needed for tie-breaking, not the full executor.
struct DagknightTieBreaker<
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

        conflict_zone_manager.fill_zone_data(all_tips.clone(), None);

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
        conflict_zone_manager.fill_zone_data(all_tips.clone(), Some(subgroup_nca));

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
    fn tie_break<'a, T>(&self, conflict_genesis: Hash, all_tips: T, subgroups: &[GroupMetadata], parents: &[Hash], k: KType) -> usize
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

struct DagknightDataIndices {
    selected_parent: u16,
    /// Indices into `tips`, ordered by conflict hierarchy with bottom-most conflicts first;
    /// reverse-iterate to get parents from latest/topmost conflicts first (as `dagknight` returns)
    reverse_conflict_ordered_parents: SmallVec<[u16; 20]>,
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
mod tests {
    use std::collections::HashMap;
    use std::str::FromStr;
    use std::{cell::RefCell, fs::File};

    use kaspa_consensus_core::blockhash::ORIGIN;
    use kaspa_consensus_core::header::Header;
    use kaspa_consensus_core::{BlockHashSet, BlueWorkType, HashMapCustomHasher};
    use kaspa_math::Uint192;
    use parking_lot::lock_api::RwLock;

    use super::*;
    use crate::model::stores::ghostdag::{GhostdagStore, GhostdagStoreReader};
    use crate::model::stores::headers::MemoryHeaderStore;
    use crate::processes::dagknight::umc_cascade_persistence::MemoryUmcCascadeStore;
    use crate::processes::ghostdag::protocol::GhostdagManager;
    use crate::processes::reachability::tests::r#gen::generate_complex_dag;
    use crate::{
        model::stores::{
            dagknight::MemoryDagknightStore, ghostdag::MemoryGhostdagStore, reachability::MemoryReachabilityStore,
            relations::MemoryRelationsStore,
        },
        processes::reachability::tests::{DagBlock, DagBuilder},
        test_helpers::generate_dot_with_chain,
    };

    #[derive(Clone)]
    pub struct DagPlan {
        genesis: u64,
        blocks: Vec<(u64, Vec<u64>)>, // All blocks other than genesis
    }

    /// Block data parsed from a JSON fixture for conflict zone tie-breaking tests.
    struct TestBlock {
        hash: Hash,
        parents: Vec<Hash>,
        blue_work: Uint192,
        bits: u32,
        blue_score: u64,
        daa_score: u64,
        selected_parent: Hash,
    }

    #[test]
    fn test_cascade() {
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();

        // Build the DAG
        {
            let plan = DagPlan {
                genesis: 1,
                blocks: vec![
                    (2, vec![1]),
                    (3, vec![1]),
                    (4, vec![2, 3]),
                    (5, vec![4]),
                    (6, vec![1]),
                    (7, vec![5, 6]),
                    (8, vec![1]),
                    (9, vec![1]),
                    (10, vec![7, 8, 9]),
                    (11, vec![1]),
                    (12, vec![11, 10]),
                ],
            };
            let mut builder = DagBuilder::new(&mut reachability, &mut relations);
            builder.init().add_block(DagBlock::genesis(plan.genesis.into()));
            for (block, parents) in plan.blocks.iter() {
                builder.add_block(DagBlock::new((*block).into(), parents.iter().map(|&i| i.into()).collect()));
            }
        }
    }

    /// This is the main body of the test.
    /// 1. It sets up the necessary stores
    /// 2. Reads the DagPlan
    /// 3. Runs DK over the blocks on it, fills the global GD store with the results
    /// 4. Generates a DOT file over that GD store showing the SPC and blocks colored
    ///    according to the global GD store
    #[allow(clippy::arc_with_non_send_sync)]
    fn run_dagknight_test(k_max: KType, plan: DagPlan, base_name: &str) {
        let genesis_hash = plan.genesis.into();

        let dk_map = RefCell::new(HashMap::new());

        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        // Global GD store. To be used for global coloring:
        let coloring_ghostdag_store = Arc::new(MemoryGhostdagStore::new());
        let headers_store = Arc::new(MemoryHeaderStore::new());

        // Global GD store. To be used for topology:
        let topology_ghostdag_store = Arc::new(MemoryGhostdagStore::new());

        let topology_gd_manager = GhostdagManager::new(
            genesis_hash,
            k_max,
            topology_ghostdag_store.clone(),
            relations.clone(),
            headers_store.clone(),
            reachability.clone(),
        );

        topology_ghostdag_store.insert(genesis_hash, Arc::new(topology_gd_manager.genesis_ghostdag_data())).unwrap();

        let coloring_gd_manager = GhostdagManager::with_custom_topology_store(
            genesis_hash,
            k_max,
            coloring_ghostdag_store.clone(),
            relations.clone(),
            headers_store.clone(),
            reachability.clone(),
            topology_ghostdag_store.clone(),
        );

        coloring_ghostdag_store.insert(genesis_hash, Arc::new(coloring_gd_manager.genesis_ghostdag_data())).unwrap();

        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));

        let dk_executor = DagknightExecutor {
            genesis_hash,
            dagknight_store: dagknight_store.clone(),
            headers_store: headers_store.clone(),
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability.clone()))),
            relations_store: relations.clone(),
            counters: Arc::new(DagknightCounters::new()),
            umc_persistence_store: Arc::new(MemoryUmcCascadeStore::new()),
        };
        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();
        let genesis = DagBlock::new(genesis_hash, vec![ORIGIN]);
        builder.add_block(genesis.clone());

        let mut tips = BlockHashSet::new();
        tips.insert(genesis.hash);

        let mut genesis_header = Header::from_precomputed_hash(genesis_hash, vec![]);
        genesis_header.bits = 0x207fffff;
        headers_store.insert(Arc::new(genesis_header));

        for block_data in &plan.blocks {
            let block_id: u64 = block_data.0;
            let block_hash = block_id.into();
            tips.insert(block_hash);

            let parent_hashes: Vec<Hash> = block_data.1.iter().map(|&a| Hash::from_u64_word(a)).collect();

            parent_hashes.iter().for_each(|ph| {
                tips.remove(ph);
            });

            let new_block = DagBlock::new(block_hash, parent_hashes.clone());

            // Pure GD for blue_work:
            let topology_gd_data = topology_gd_manager.ghostdag(&new_block.parents);

            let DagknightData { selected_parent, .. } = dk_executor.dagknight(&new_block.parents);

            // Maintain global coloring based on DK megachain selected parent:
            let gd_data = coloring_gd_manager.incremental_coloring(&new_block.parents, selected_parent);

            builder.add_block_with_selected_parent(new_block, selected_parent);

            let mut curr_header = Header::from_precomputed_hash(block_hash, parent_hashes);
            curr_header.bits = 0x207fffff;
            curr_header.daa_score = gd_data.blue_score;
            curr_header.blue_score = gd_data.blue_score;
            curr_header.blue_work = topology_gd_data.blue_work;

            topology_ghostdag_store.insert(block_hash, Arc::new(topology_gd_data)).unwrap();
            coloring_ghostdag_store.insert(block_hash, Arc::new(gd_data)).unwrap();

            headers_store.insert(Arc::new(curr_header));
        }

        let tip_hashes = tips.iter().copied().collect_vec();
        let virtual_hash = Hash::from_u64_word(plan.blocks.last().unwrap().0 + 1);
        let virtual_block = DagBlock::new(virtual_hash, tip_hashes.clone());
        let DagknightData { selected_parent, .. } = dk_executor.dagknight(&virtual_block.parents.clone());
        // let selected_parent = dk_data.selected_parent;
        let gd_data = coloring_gd_manager.incremental_coloring(&tip_hashes, selected_parent);
        println!("virtual_block: {} | sp: {}", virtual_block.hash, selected_parent);
        builder.add_block_with_selected_parent(virtual_block, selected_parent);
        coloring_ghostdag_store.insert(virtual_hash, Arc::new(gd_data)).unwrap();

        // let blues = BlockHashSet::new();
        let mut reds = BlockHashSet::new();

        // Collect chain nodes during VSPC traversal
        let mut chain_nodes = BlockHashSet::new();
        let mut curr = virtual_hash;
        chain_nodes.insert(curr);

        while curr != genesis.hash {
            let mergeset_reds = coloring_ghostdag_store.get_mergeset_reds(curr).unwrap();
            mergeset_reds.iter().for_each(|mrr| {
                reds.insert(*mrr);
            });

            let chain_parent = reachability.get_chain_parent(curr);
            println!("{} <- {}", chain_parent.to_le_u64()[3], curr.to_le_u64()[3]);
            chain_nodes.insert(chain_parent);
            curr = chain_parent;
        }

        // Generate DOT file with chain nodes as double circles
        let mut all_blocks = vec![(plan.genesis, vec![])];
        all_blocks.extend(plan.blocks.clone());
        all_blocks.push((virtual_hash.to_le_u64()[3], tips.iter().map(|h| h.to_le_u64()[3]).collect_vec()));
        generate_dot_with_chain(&all_blocks, &chain_nodes, reds, base_name).expect("Failed to generate DOT file");
    }

    #[test]
    fn test_dag_dk_sample() {
        let plan = DagPlan {
            genesis: 1,
            blocks: vec![
                (2, vec![1]),
                (3, vec![2]),
                (4, vec![3]),
                (5, vec![4]),
                (6, vec![5]),
                (7, vec![6]),
                (8, vec![7]),
                (9, vec![7]),
                (10, vec![8, 9]),
                (11, vec![10]),
                (12, vec![1]),
                (13, vec![12]),
                (14, vec![13]),
                (15, vec![14]),
                (16, vec![15]),
                (17, vec![6, 16]),
            ],
        };

        run_dagknight_test(0, plan, "dag_bps_whitepaper_sample");
    }

    #[test]
    fn test_dag_from_json() {
        // Test the Task 0 implementation here
        let json_filename = "dag_bps_2.json";
        let file = File::open(json_filename).expect("Unable to open JSON file");
        let json_data: serde_json::Value = serde_json::from_reader(file).expect("Unable to parse JSON");

        let genesis = json_data["genesis"].as_u64().expect("Genesis is not a number");
        let blocks = json_data["blocks"].as_array().expect("Blocks is not an array");

        // Construct DagPlan from JSON data
        let dag_plan = DagPlan {
            genesis,
            blocks: blocks
                .iter()
                .map(|block| {
                    let id = block["id"].as_u64().unwrap();
                    let parents = block["parents"].as_array().unwrap().iter().map(|p| p.as_u64().unwrap()).collect();
                    (id, parents)
                })
                .chain(vec![(60, vec![1]), (61, vec![1]), (62, vec![60, 61]), (63, vec![60, 61]), (70, vec![50, 51, 63])])
                .collect(),
        };

        // print the data
        println!("Genesis: {}", dag_plan.genesis);
        println!("Blocks: {}", dag_plan.blocks.len());

        // Sample here is 2BPS. K = 31
        run_dagknight_test(31, dag_plan, "dag_bps_2");
    }

    #[test]
    fn test_complex_dag() {
        let (genesis, mut blocks) = generate_complex_dag(0.1, 10.0, 50);
        let (_, attacker_blocks) = generate_complex_dag(0.1, 10.0, 40);

        // Make the attacker blocks still point to the original genesis and adjust their labels
        let mut attacker_blocks = attacker_blocks
            .iter()
            .map(|(block, parents)| {
                let block = if *block == genesis { *block } else { block + 100 };
                let parents = parents.iter().map(|&p| if p == genesis { p } else { p + 100 }).collect_vec();

                (block, parents)
            })
            .collect_vec();

        blocks.append(&mut attacker_blocks);

        let plan = DagPlan { genesis, blocks };

        run_dagknight_test(5, plan, "dag_complex");
    }

    #[test]
    fn test_monitonicity_simple() {
        // SETUP:
        let genesis_hash = 1.into();

        let dk_map = RefCell::new(HashMap::new());

        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();

        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut genesis_header = Header::from_precomputed_hash(genesis_hash, vec![]);
        genesis_header.bits = 0x207fffff;
        headers_store.insert(Arc::new(genesis_header));
        // Global GD store. To be used for topology:
        let topology_ghostdag_store = Arc::new(MemoryGhostdagStore::new());

        let topology_gd_manager = GhostdagManager::new(
            genesis_hash,
            5,
            topology_ghostdag_store.clone(),
            relations.clone(),
            headers_store.clone(),
            reachability.clone(),
        );

        topology_ghostdag_store.insert(genesis_hash, Arc::new(topology_gd_manager.genesis_ghostdag_data())).unwrap();

        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));

        let dk_executor = DagknightExecutor {
            genesis_hash,
            dagknight_store: dagknight_store.clone(),
            headers_store: headers_store.clone(),
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability.clone()))),
            relations_store: relations.clone(),
            counters: Arc::new(DagknightCounters::new()),
            umc_persistence_store: Arc::new(MemoryUmcCascadeStore::new()),
        };
        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();
        let genesis = DagBlock::new(genesis_hash, vec![ORIGIN]);
        builder.add_block(genesis.clone());

        // Add blocks 2 and 3 and insert headers/ghostdag entries.
        // We'll use a small helper closure to reduce repetition when adding a block and its header.
        let mut add_block_with_header = |id: u64, parents: Vec<Hash>| {
            let current_hash = id.into();
            let DagknightData { selected_parent, .. } = dk_executor.dagknight(&parents);
            builder.add_block_with_selected_parent(DagBlock::new(current_hash, parents.clone()), selected_parent);
            let gd = topology_gd_manager.ghostdag(&parents);

            let mut header = Header::from_precomputed_hash(current_hash, parents);
            header.bits = 0x207fffff;
            header.daa_score = gd.blue_score;
            header.blue_score = gd.blue_score;
            header.blue_work = gd.blue_work;
            headers_store.insert(Arc::new(header));
            topology_ghostdag_store.insert(current_hash, Arc::new(gd)).unwrap();

            current_hash
        };

        // TEST BEGINS HERE:
        // This test follows the example described in the DK paper section 2.6.6
        //     1
        //    ↙ ↘
        //   2   3
        //   |   |\ \ \ \
        //   ↓   ↓ ↓ ↓ ↓ ↓
        //   9   4 5 6 7 8
        //
        let hash_of_2 = add_block_with_header(2, vec![genesis_hash]);
        let hash_of_3 = add_block_with_header(3, vec![genesis_hash]);

        let DagknightData { selected_parent: virtual_sp, .. } = dk_executor.dagknight(&[hash_of_2, hash_of_3]);
        println!("virtual sp: {}", virtual_sp);

        let other_tip = if hash_of_2 == virtual_sp { hash_of_3 } else { hash_of_2 };
        let mut tips = vec![];

        // Raise the rank of the selected tip of previos selected parent by pointing multiple blocks to it
        for i in 4..9 {
            let current_hash = add_block_with_header(i, vec![virtual_sp]);
            tips.push(current_hash);
        }

        // Add just one tip to previously unselected parent
        let hash_of_9 = add_block_with_header(9, vec![other_tip]);
        tips.push(hash_of_9);

        let DagknightData { selected_parent: new_sp_virtual, .. } = dk_executor.dagknight(&tips);
        println!("new virtual sp: {}", new_sp_virtual);

        assert!(
            reachability.is_chain_ancestor_of(virtual_sp, new_sp_virtual),
            "The selected parent chain changed after attacker raised the rank of previously selected tip"
        )
    }

    #[test]
    fn test_parent_ordering_stability() {
        let genesis_hash = Hash::from_u64_word(1);
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let headers_store = Arc::new(MemoryHeaderStore::new());

        let dk_map = RefCell::new(HashMap::new());

        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));

        let dk_executor = DagknightExecutor {
            genesis_hash,
            dagknight_store: dagknight_store.clone(),
            headers_store: headers_store.clone(),
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability.clone()))),
            relations_store: relations.clone(),
            counters: Arc::new(DagknightCounters::new()),
            umc_persistence_store: Arc::new(MemoryUmcCascadeStore::new()),
        };

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

        let json_filename = "test_parent_ordering_stability.json";
        let file = File::open(json_filename).expect("Unable to open JSON file");
        let json_data: serde_json::Value = serde_json::from_reader(file).expect("Unable to parse JSON");

        let tips: Vec<Hash> = json_data["tips"].as_array().unwrap().iter().map(|t| prefixed_hash(t.as_str().unwrap())).collect();

        let blocks = json_data["blocks"].as_array().expect("Blocks is not an array");

        let test_blocks: Vec<TestBlock> = blocks
            .iter()
            .map(|block| {
                let hash = prefixed_hash(block["id"].as_str().unwrap());
                let parents: Vec<Hash> = if block["parents"].as_array().map(|a| a.is_empty()).unwrap_or(false) {
                    vec![ORIGIN]
                } else {
                    block["parents"].as_array().unwrap().iter().map(|p| prefixed_hash(p.as_str().unwrap())).collect()
                };
                let blue_work = Uint192::from_u64(block["blue_work"].as_str().unwrap().parse::<u64>().unwrap());
                let bits = u32::from_str_radix(block["bits"].as_str().unwrap(), 16).unwrap();
                let blue_score = block["blue_score"].as_u64().unwrap();
                let daa_score = block["daa_score"].as_u64().unwrap();
                let selected_parent = if block["selected_parent"].is_null() {
                    ORIGIN
                } else {
                    prefixed_hash(block["selected_parent"].as_str().unwrap())
                };
                TestBlock { hash, parents, blue_work, bits, blue_score, daa_score, selected_parent }
            })
            .collect();

        let mut test_blocks = test_blocks;

        test_blocks.sort_by_key(|block| block.blue_work);

        for block in &test_blocks {
            add_block(
                block.hash,
                block.parents.clone(),
                block.blue_work,
                block.bits,
                block.blue_score,
                block.daa_score,
                block.selected_parent,
            );
        }

        let mut parents = tips.clone();
        let base_result = dk_executor.dagknight(&parents);

        parents.sort();
        let sorted_result = dk_executor.dagknight(&parents);

        assert_eq!(
            base_result.selected_parent, sorted_result.selected_parent,
            "Selected parent must be the same regardless of parent order"
        );
    }

    fn prefixed_hash(s: &str) -> Hash {
        let mut hex = [b'0'; 64];
        hex[..s.len()].copy_from_slice(s.as_bytes());
        Hash::from_str(std::str::from_utf8(&hex).unwrap()).expect("Invalid hash string")
    }

    /// Duplicate parents [T1, T1] must NOT panic the DagKnight algorithm.
    ///
    /// Before the fix: this panics at the `assert_ne!` in `dagknight_pure` because
    /// `common_chain_ancestor([T1, T1])` returns the same conflict_genesis in the shortcut
    /// path's single-group case, violating the "conflict genesis must strictly advance" invariant.
    #[test]
    fn test_duplicate_parents_two_same() {
        // genesis -> T1
        let genesis_hash = Hash::from_u64_word(1);
        let t1_hash = Hash::from_u64_word(2);

        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();
        builder.add_block(DagBlock::new(genesis_hash, vec![ORIGIN]));
        builder.add_block(DagBlock::new(t1_hash, vec![genesis_hash]));

        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut genesis_header = Header::from_precomputed_hash(genesis_hash, vec![]);
        genesis_header.bits = 0x207fffff;
        headers_store.insert(Arc::new(genesis_header));
        let mut t1_header = Header::from_precomputed_hash(t1_hash, vec![genesis_hash]);
        t1_header.bits = 0x207fffff;
        t1_header.daa_score = 1;
        headers_store.insert(Arc::new(t1_header));

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));
        let dk_executor = DagknightExecutor {
            genesis_hash,
            dagknight_store,
            headers_store,
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability))),
            relations_store: relations,
            counters: Arc::new(DagknightCounters::new()),
            umc_persistence_store: Arc::new(MemoryUmcCascadeStore::new()),
        };

        // Two identical parents: [T1, T1]
        let duplicate_parents: Vec<Hash> = vec![t1_hash, t1_hash];
        let result = dk_executor.dagknight(&duplicate_parents);
        assert_eq!(result.selected_parent, t1_hash);
    }

    /// Duplicate parents [T1, T1, T2] must NOT panic the DagKnight algorithm.
    ///
    /// T1 and T2 are independent tips (both children of genesis), so they are in the anticone.
    /// `.unique()` deduplicates [T1, T1, T2] to [T1, T2], which forms two agreement groups,
    /// so the shortcut path is not taken.
    #[test]
    fn test_duplicate_parents_three_with_one_dup() {
        // genesis -> T1, genesis -> T2 (independent tips, in anticone)
        let genesis_hash = Hash::from_u64_word(1);
        let t1_hash = Hash::from_u64_word(2);
        let t2_hash = Hash::from_u64_word(3);

        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();
        builder.add_block(DagBlock::new(genesis_hash, vec![ORIGIN]));
        builder.add_block(DagBlock::new(t1_hash, vec![genesis_hash]));
        builder.add_block(DagBlock::new(t2_hash, vec![genesis_hash]));

        let headers_store = Arc::new(MemoryHeaderStore::new());
        let mut genesis_header = Header::from_precomputed_hash(genesis_hash, vec![]);
        genesis_header.bits = 0x207fffff;
        headers_store.insert(Arc::new(genesis_header));
        let mut t1_header = Header::from_precomputed_hash(t1_hash, vec![genesis_hash]);
        t1_header.bits = 0x207fffff;
        t1_header.daa_score = 1;
        headers_store.insert(Arc::new(t1_header));
        let mut t2_header = Header::from_precomputed_hash(t2_hash, vec![genesis_hash]);
        t2_header.bits = 0x207fffff;
        t2_header.daa_score = 1;
        headers_store.insert(Arc::new(t2_header));

        let dk_map = RefCell::new(HashMap::new());
        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));
        let dk_executor = DagknightExecutor {
            genesis_hash,
            dagknight_store,
            headers_store,
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability))),
            relations_store: relations,
            counters: Arc::new(DagknightCounters::new()),
            umc_persistence_store: Arc::new(MemoryUmcCascadeStore::new()),
        };

        // Three parents where first two are identical: [T1, T1, T2]
        let duplicate_parents: Vec<Hash> = vec![t1_hash, t1_hash, t2_hash];
        let result = dk_executor.dagknight(&duplicate_parents);

        // Selected parent should be one of the unique parents
        assert!(
            result.selected_parent == t1_hash || result.selected_parent == t2_hash,
            "selected parent {:?} should be one of {:?} or {:?}",
            result.selected_parent,
            t1_hash,
            t2_hash
        );
    }

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
            DagknightTieBreaker {
                dagknight_store: &self.dagknight_store,
                headers_store: &self.headers_store,
                relations_store: self.relations_store.clone(),
                reachability_service: self.reachability_service.clone(),
            }
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
        let tie_breaker = DagknightTieBreaker {
            dagknight_store: &dagknight_store,
            headers_store: &headers_store,
            relations_store,
            reachability_service,
        };

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
