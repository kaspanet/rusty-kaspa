use crate::processes::dagknight::tie_breaking::DagknightTieBreaker;
#[cfg(feature = "baseline-debugging")]
use crate::processes::dagknight::umc_baseline::BaselineUmcVoter;
use crate::processes::dagknight::umc_voting::{UmcVoter, UmcVotingContext};
use itertools::Itertools;
use kaspa_consensus_core::KType;
use kaspa_core::debug;
use kaspa_hashes::Hash;
use smallvec::SmallVec;
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
            DagknightCounters, Group, GroupMetadata,
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

#[derive(Clone)]
pub struct DagknightData {
    pub selected_parent: Hash,               // The selected parent for this call
    pub conflict_ordered_parents: Vec<Hash>, // The rest of the parents, ordered by conflict hierarchy (parents from latest/topmost conflicts first)
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
    // TODO[DK]: return the conflict-ordered parents as u16 indices into `parents` instead of collecting the hashes
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

    /// Baseline UMC cascade voting: naive reference impl of paper Algorithm 6 (work-weighted),
    /// used to cross-check `SegmentTreeUmcVoter` (see the comparison in
    /// `select_parent_from_k_colouring`).
    #[cfg(feature = "baseline-debugging")]
    fn baseline_umc_cascade_voting(
        &self,
        conflict_genesis: Hash,
        next_chain_ancestor: &Hash,
        virtual_gd: GhostdagData,
        k: KType,
        conflict_zone_manager: &ConflictZoneManager<&C, &O, &D, &MTReachabilityService<R>>,
    ) -> CascadeResult {
        // TODO[DK]: use references instead of clones once the store traits are implemented for &T
        let voter = BaselineUmcVoter::new(self.headers_store.clone(), self.reachability_service.clone());
        let ctx = UmcVotingContext {
            conflict_genesis,
            next_chain_ancestor,
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
        next_chain_ancestor: &Hash,
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
            next_chain_ancestor,
            virtual_gd: &virtual_gd,
            k,
            coloring_reader: conflict_zone_manager,
        };
        voter.vote(&ctx)
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

        // TODO[DK]: use references for relations/reachability as well once the traits are implemented for &T
        DagknightTieBreaker::new(
            &*self.dagknight_store,
            &*self.headers_store,
            self.relations_store.clone(),
            self.reachability_service.clone(),
        )
        .tie_break(
            conflict_genesis,
            agreement_grouping.iter().map(|group| &parents[group.parent as usize]),
            subgroups,
            parents,
            mutual_k,
        )
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
                        subgroup[0].common_ancestor,
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

    /// Applies a coloring to the conflict zone, and determines if the
    /// coloring represents a majority over "g" only (as opposed to full UMC)
    /// TODO[DK]: Implement full UMC cascade voting after coloring
    fn select_parent_from_k_colouring<'a, P, T>(
        &self,
        conflict_genesis: Hash,
        subgroup_nca: Hash,
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

        conflict_zone_manager.fill_zone_data(subgroup.clone(), subgroup_nca);

        // selected a parent in this subgroup => Conditioned upon virtual agreeing with this subgroup
        let subgroup_virtual_sp = conflict_zone_manager.find_selected_parent(subgroup);
        let virtual_gd = conflict_zone_manager.k_colouring(all_tips, k_to_check, Some(subgroup_virtual_sp));

        let cascade_result =
            self.umc_cascade_voting(conflict_genesis, &subgroup_nca, virtual_gd.clone(), k_to_check, &conflict_zone_manager);

        #[cfg(feature = "baseline-debugging")]
        {
            // Compare baseline (per-blue recursive) against cascade (global virtual score)
            // These use different acceptance criteria and are not expected to always agree.
            // The baseline is Algorithm 6 from the paper; the cascade is the optimized implementation.
            let baseline_result = self.baseline_umc_cascade_voting(
                conflict_genesis,
                &subgroup_nca,
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
    use kaspa_consensus_core::{BlockHashSet, HashMapCustomHasher};
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
}
