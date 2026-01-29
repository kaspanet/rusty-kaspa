use std::{
    cmp::{Ordering, Reverse},
    collections::BinaryHeap,
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashKTypeMap, HashMapCustomHasher, KType,
    blockhash::{self, BlockHashes},
};
use kaspa_core::debug;
use kaspa_database::prelude::StoreResultExt;
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use parking_lot::RwLock;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            children::ChildrenStore,
            dagknight::{DagknightStore, DagknightStoreReader},
            ghostdag::{GhostdagData, GhostdagStoreReader},
            headers::HeaderStoreReader,
            reachability::{MemoryReachabilityStore, ReachabilityStore, ReachabilityStoreReader},
            relations::{MemoryRelationsStore, RelationsStore, RelationsStoreReader},
        },
    },
    processes::{
        dagknight::manager::ConflictZoneManager, difficulty::calc_work, ghostdag::ordering::SortableBlock,
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
    O: GhostdagStoreReader,
    D: HeaderStoreReader + 'static,
    E: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> {
    pub genesis_hash: Hash,
    pub dagknight_store: Arc<C>,
    pub ghostdag_store: Arc<O>,
    pub headers_store: Arc<D>,
    pub relations_store: Arc<RwLock<E>>,
    pub reachability_service: MTReachabilityService<R>,
}

impl<
    C: DagknightStore + DagknightStoreReader,
    O: GhostdagStoreReader,
    D: HeaderStoreReader,
    E: RelationsStoreReader + Clone,
    R: ReachabilityStoreReader + Clone,
> DagknightExecutor<C, O, D, E, R>
{
    pub fn dagknight(&self, parents: &[Hash]) -> Hash {
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

        let current_parents = parents.to_vec();

        // g = find LCCA
        let mut conflict_genesis = self.common_chain_ancestor(parents);
        let mut curr_subgroup = current_parents;

        while curr_subgroup.len() > 1 {
            let group_map = curr_subgroup
                .iter()
                .copied()
                .into_group_map_by(|&parent| self.reachability_service.get_next_chain_ancestor(parent, conflict_genesis));

            // Shortcut condition to avoid doing unnecessary work
            if group_map.len() == 1 {
                // There is exactly one group, we don't rank anymore.
                let (&curr_conflict_genesis, subgroup) = group_map.iter().next().unwrap();
                curr_subgroup = subgroup.to_vec();
                conflict_genesis = curr_conflict_genesis;
                continue;
            }

            // Pick a "winner" among these subgroups
            let (winning_conflict_genesis, winning_subgroup) = group_map
                .iter()
                .map(|(curr_conflict_genesis, subgroup)| {
                    let rank_value = self.rank(conflict_genesis, subgroup, &curr_subgroup);
                    (rank_value, curr_conflict_genesis, subgroup)
                })
                .min_by(|(a, _, _), (b, _, _)| a.cmp(b))
                .map(|(rank, conflict_genesis, subgroup)| {
                    debug!("Winning rank value: k = {} | sp = {:#?}", rank.k, rank.selected_parent.hash);
                    (*conflict_genesis, subgroup)
                })
                .unwrap();

            curr_subgroup = winning_subgroup.to_vec();
            conflict_genesis = winning_conflict_genesis;
        }
        assert_eq!(1, curr_subgroup.len(), "Expected dagknight to have only a single parent at the end");

        curr_subgroup[0]
    }

    fn common_chain_ancestor(&self, parents: &[Hash]) -> Hash {
        /*
           Notes:
               - ignore parents not agreeing on the pruning point as a chain block
               - optimize for shortest path
               - optimize with index
        */

        let start = parents[0];

        if start == self.genesis_hash {
            return self.genesis_hash;
        }

        for cb in self.reachability_service.default_backward_chain_iterator(start).skip(1) {
            if self.reachability_service.is_chain_ancestor_of_all(cb, &parents[1..]) {
                return cb;
            }
        }

        panic!("")
    }

    fn _umc_cascade_voting(&self) {
        /*
            inputs: G, U, d
            output: does U have a subset U' s.t. U' is d-UMC of G
                    where d-UMC means that each block in U' is majority covered by U' (up to d)

            sketch 1:
                maintain the blue/total past sizes and blue/total anticone sizes for each blue block
            problems:
                1. keeping the anticone size can be costly (a single attacker block with a huge anticone would dirty many entries)
                2. challenging to reason about blue blocks which can be treated as red (U / U')
                3. plus does not benefit from the above


        */
    }

    /// Follows the Calculate-Rank algorithm in the DK paper
    /// For now, this is missing the concept of representatives, and instead subgroup_tips == representatives
    /// TODO[DK]: Properly implement representatives
    ///
    /// Currently returns both the Rank and a selected parent (deviates from the paper) since the tie breaking logic
    /// in the caller is simply using blue_work + hash to break ties between subgroups
    /// TODO[DK]: Remove selected_parent from the RankValue and properly implement Tie-Breaking
    ///
    /// K-searching logic:
    /// 1. Search for an upper bound using powers of 2
    ///    1.1 For each unsuccessful step along the way, move the lower bound k up as well
    ///    1.2 Also exits if lkg_k is a max
    /// 2. Binary search between lower bound k and lkg_k
    fn rank(&self, conflict_genesis: Hash, subgroup: &[Hash], all_tips: &[Hash]) -> RankValue {
        // for k in 0, 1, ..., infinity:
        // tie breaking is assumed to be by comparing selected_parent
        let mut selected_parent = None;

        // The steps through which we'll check for the upper bound.
        // This will cover the powers of 2, starting from 2^0
        let mut increments = 1;

        // Find upper bound k (good k value that satisfies the condition):
        let mut lkg_k = 0;
        // binary search lower bound. upper bound is lkg_k
        let mut lower_k: KType = 0;
        let mut found_lkg = false;

        let mut steps = 0;

        while !found_lkg && lkg_k != u16::MAX {
            steps += 1;
            debug!("Finding upper bound k = {}", lkg_k);
            let curr_selected_parent = self.select_parent_from_k_colouring(conflict_genesis, subgroup, all_tips, lkg_k);

            if curr_selected_parent.is_some() {
                debug!("Found a valid sp at upper bound k = {}", lkg_k);
                selected_parent = curr_selected_parent;
                found_lkg = true;
            } else {
                // Move the lower bound up to start the binary search further later
                lower_k = lkg_k;
                // increment is powers of 2: 1, 2, 4, 8, 16...
                lkg_k = increments;
                increments = increments.saturating_mul(2);
            }
        }

        while lower_k < lkg_k {
            steps += 1;
            let k_to_check = lower_k + ((lkg_k - lower_k) / 2);

            let curr_selected_parent = self.select_parent_from_k_colouring(conflict_genesis, subgroup, all_tips, k_to_check);
            let sp_found = curr_selected_parent.is_some();

            if sp_found {
                debug!("Found a valid sp at mid k = {} | low = {} | hi = {}", k_to_check, lower_k, lkg_k);
                lkg_k = k_to_check;
                selected_parent = curr_selected_parent;
            } else {
                lower_k = k_to_check + 1;
            }
        }

        debug!("Steps taken: {steps}");

        RankValue { k: lkg_k, selected_parent: selected_parent.unwrap() }
    }

    /// Applies a coloring to the conflict zone, and determines if the
    /// coloring represents a majority over "g" only (as opposed to full UMC)
    /// TODO[DK]: Implement full UMC cascade voting after coloring
    fn select_parent_from_k_colouring(
        &self,
        conflict_genesis: Hash,
        subgroup: &[Hash],
        all_tips: &[Hash],
        k_to_check: KType,
    ) -> Option<SortableBlock> {
        let conflict_zone_manager = self.fill_conflict_zone_data(conflict_genesis, all_tips, k_to_check);

        // selected a parent in this subgroup => Conditioned upon virtual agreeing with this subgroup
        let subgroup_virtual_sp = conflict_zone_manager.find_selected_parent(subgroup.iter().copied());
        let virtual_gd = conflict_zone_manager.k_colouring(all_tips, k_to_check, Some(subgroup_virtual_sp));

        // Add deficit logic here => sqrt(k) * work_of_some_block
        // TODO[DK]: Right now deficit logic uses conflict genesis. Maybe there's a better value to use like an average.
        // Figure it out
        let deficit_work_basis = calc_work(self.headers_store.get_bits(conflict_genesis).unwrap());
        let deficit = Uint192::from_u64(k_to_check.isqrt() as u64) * deficit_work_basis;

        let blue_block_work = virtual_gd.blue_work;
        let mut gray_block_work = Uint192::ZERO;
        let mut red_block_work = Uint192::ZERO;
        let next_chain_ancestor_of_subgroup = self.reachability_service.get_next_chain_ancestor(subgroup[0], conflict_genesis);

        // TODO[DK]: Iterate through the VSPC red mergeset to determine red/gray work
        let mut curr_gd = Arc::new(virtual_gd);

        while curr_gd.selected_parent != conflict_genesis {
            for &red_block in curr_gd.mergeset_reds.iter() {
                let red_block_bits = self.headers_store.get_bits(red_block).unwrap();
                let red_work = calc_work(red_block_bits);

                if self.reachability_service.is_chain_ancestor_of(next_chain_ancestor_of_subgroup, red_block) {
                    gray_block_work = gray_block_work + red_work;
                } else {
                    red_block_work = red_block_work + red_work;
                }
            }

            curr_gd = conflict_zone_manager.get_data(curr_gd.selected_parent).unwrap();
        }

        debug!(
            "k = {} | blue work = {} | gray work = {} | red work = {} | deficit = {}",
            k_to_check, blue_block_work, gray_block_work, red_block_work, deficit
        );
        if blue_block_work + deficit > red_block_work {
            // Michael: here is where cascade voting eventually belongs
            // With this "k" value, our selected parent covers at least half the total region's work (minus some deficit)
            Some(SortableBlock { hash: subgroup_virtual_sp, blue_work: blue_block_work })
        } else {
            None
        }
    }

    // Calculates the rank of the subgroup over the region: <root, tips>
    // root = conflict genesis
    // subgroup = the current subgroup
    // tips = all tips in this conflict. part of which is the subgroup
    //
    // Returns the conflict zone manager which gives access to the coloring data of the conflict zone
    fn fill_conflict_zone_data(&self, root: Hash, tips: &[Hash], ghostdag_k: KType) -> ConflictZoneManager<C, E, R, D> {
        let reachability_service = self.reachability_service.clone();
        let relations_store = self.relations_store.read();
        let relations_service = FutureIntersectRelations::new(relations_store.clone(), reachability_service.clone(), root);
        let conflict_manager = ConflictZoneManager::new(
            ghostdag_k,
            root,
            self.dagknight_store.clone(),
            relations_service.clone(),
            reachability_service.clone(),
            self.headers_store.clone(),
        );

        // Note there is no need to initialize origin since we have a single root
        if !conflict_manager.has(root) {
            conflict_manager
                .insert(
                    root,
                    Arc::new(GhostdagData::new(
                        0,
                        Default::default(),
                        blockhash::ORIGIN,
                        BlockHashes::new(Vec::new()),
                        BlockHashes::new(Vec::new()),
                        HashKTypeMap::new(BlockHashMap::new()),
                    )),
                )
                .unwrap();
        }

        let mut topological_heap: BinaryHeap<_> = Default::default();

        let mut visited = BlockHashSet::new();

        // TODO: Determine starting roots by backward iterating from subgroup tips to root
        // and stopping at the last blocks without GD data yet
        // TODO: Right now it's initializing from the root, but really it should initialized from the saved tips we know
        // for the k-cluster with this root (since we're tracking tips). This way, the BFS starts only from the tips if
        // we see another conflict for this root+k.
        topological_heap
            .push(Reverse(SortableBlock { hash: root, blue_work: self.headers_store.get_header(root).unwrap().blue_work }));

        loop {
            let Some(current) = topological_heap.pop() else {
                break;
            };
            let current_hash = current.0.hash;
            if !visited.insert(current_hash) {
                continue;
            }

            if !reachability_service.is_dag_ancestor_of_any(current_hash, &mut tips.iter().copied()) {
                // We don't care about blocks in the antipast of tips
                continue;
            }

            if !conflict_manager.has(current_hash) {
                // TODO[DK]: Impement proper k-colouring lines 9-10 from DK paper if still needed

                // Implements k-colouring assuming free_search is always false
                let parents = &relations_service.get_parents(current_hash).unwrap();
                let next_chain_ancestor_of_current = reachability_service.get_next_chain_ancestor(current_hash, root);
                let agreeing_parents = parents
                    .iter()
                    .copied()
                    .filter(|&p| {
                        next_chain_ancestor_of_current == current_hash
                            || self.reachability_service.is_chain_ancestor_of(next_chain_ancestor_of_current, p)
                    })
                    .collect::<Vec<_>>();
                assert!(
                    !agreeing_parents.is_empty(),
                    "Expected at least one agreeing parent | current: {:#?} | parents: {:#?}",
                    current_hash,
                    parents
                );

                let selected_parent = conflict_manager.find_selected_parent(agreeing_parents.iter().copied());
                let current_gd = conflict_manager.k_colouring(parents, ghostdag_k, Some(selected_parent));

                conflict_manager.insert(current_hash, Arc::new(current_gd)).optional().unwrap();
            }

            for child in relations_service.get_children(current_hash).unwrap().read().iter().copied() {
                if !self.reachability_service.is_chain_ancestor_of(root, child) {
                    debug!("Skipping child not a chain descendant of root | root: {:#?} | child: {:#?}", root, child);
                    continue;
                }
                topological_heap
                    .push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
            }
        }

        conflict_manager
    }
}

mod ct {
    use super::*;
    use std::{
        cmp::Ordering,
        collections::{
            BTreeSet,
            hash_map::Entry::{Occupied, Vacant},
        },
    };

    /// BTree entry
    #[derive(Eq, Clone)]
    pub struct CascadeTreeEntry {
        pub hash: Hash,
        pub floor: i64,
    }

    impl CascadeTreeEntry {
        pub fn new(hash: Hash, floor: i64) -> Self {
            Self { hash, floor }
        }
    }

    impl PartialEq for CascadeTreeEntry {
        fn eq(&self, other: &Self) -> bool {
            self.floor == other.floor && self.hash == other.hash
        }
    }

    impl PartialOrd for CascadeTreeEntry {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }

    impl Ord for CascadeTreeEntry {
        fn cmp(&self, other: &Self) -> Ordering {
            self.floor.cmp(&other.floor).then_with(|| self.hash.cmp(&other.hash))
        }
    }

    #[derive(Default)]
    pub struct CascadeTree {
        btree: BTreeSet<CascadeTreeEntry>,
        rev_index: BlockHashMap<i64>,

        // Exact counters
        past_blues: BlockHashMap<u64>,
        past_reds: BlockHashMap<u64>,
        anticone_blues: BlockHashMap<u64>,

        /// Anticone reds lower bound
        arlb: BlockHashMap<u64>,
    }

    impl CascadeTree {
        /// Insert a new block.
        pub fn insert(
            &mut self,
            hash: Hash,
            past_blues: u64,
            past_reds: u64,
            anticone_blues: u64,
            anticone_reds_lower_bound: u64,
        ) -> bool {
            match self.past_blues.entry(hash) {
                Occupied(_) => return false,
                Vacant(e) => e.insert(past_blues),
            };
            self.past_reds.insert(hash, past_reds).is_none().then_some(()).unwrap();
            self.anticone_blues.insert(hash, anticone_blues).is_none().then_some(()).unwrap();
            self.arlb.insert(hash, anticone_reds_lower_bound).is_none().then_some(()).unwrap();

            let floor = past_reds as i64 + anticone_reds_lower_bound as i64 - past_blues as i64 - anticone_blues as i64;
            self.btree.insert(CascadeTreeEntry::new(hash, floor)).then_some(()).unwrap();
            self.rev_index.insert(hash, floor).is_none().then_some(()).unwrap();

            true
        }

        /// Update `anticone_blues` of an existing block.
        ///
        /// TODO: Result
        pub fn _update_anticone_blues(&mut self, hash: Hash, anticone_blues: u64) {
            let prev_floor = self.rev_index[&hash];
            let prev_anticone_blues = self.anticone_blues.insert(hash, anticone_blues).unwrap();
            let new_floor = prev_floor - (anticone_blues as i64 - prev_anticone_blues as i64);
            self.btree.remove(&CascadeTreeEntry::new(hash, prev_floor)).then_some(()).unwrap();
            self.btree.insert(CascadeTreeEntry::new(hash, new_floor)).then_some(()).unwrap();
            self.rev_index.insert(hash, new_floor);
            assert!(anticone_blues > prev_anticone_blues);
        }

        /// Update `anticone_reds_lower_bound` of an existing block.
        ///
        /// TODO: Result
        pub fn _update_anticone_reds_lower_bound(&mut self, hash: Hash, anticone_reds_lower_bound: u64) {
            let prev_floor = self.rev_index[&hash];
            let prev_arlb = self.arlb.insert(hash, anticone_reds_lower_bound).unwrap();
            let new_floor = prev_floor + (anticone_reds_lower_bound as i64 - prev_arlb as i64);
            self.btree.remove(&CascadeTreeEntry::new(hash, prev_floor)).then_some(()).unwrap();
            self.btree.insert(CascadeTreeEntry::new(hash, new_floor)).then_some(()).unwrap();
            self.rev_index.insert(hash, new_floor);
            assert!(anticone_reds_lower_bound > prev_arlb);
        }

        // pub fn peek_min(&self) -> CascadeTreeEntry {
        //     self.btree.first().cloned().unwrap()
        // }
    }
}

use ct::CascadeTree;

/// Cascade related data structures
#[derive(Default)]
pub struct CascadeDast {
    /// TEMP: the full DAG (as of this processing point)
    g: BlockHashSet,

    /// Blue set
    blueset: BlockHashSet,

    // B tree ordered by floor values
    tree: CascadeTree,
}

pub struct TraversalContext<'a, T: ReachabilityStore + ?Sized, S: RelationsStore + ChildrenStore + ?Sized> {
    /// The reachability oracle
    _oracle: &'a T,
    /// The relations oracle (local DAG area)
    _relations: &'a S,
}

impl<'a, T: ReachabilityStore + ?Sized, S: RelationsStore + ChildrenStore + ?Sized> TraversalContext<'a, T, S> {
    pub fn new(reachability: &'a T, _relations: &'a S) -> Self {
        Self { _oracle: reachability, _relations }
    }
}

pub type MemTraversalContext<'a> = TraversalContext<'a, MemoryReachabilityStore, MemoryRelationsStore>;

pub enum BlockColouring {
    Blue { anticone_blues: u64, past: u64 },
    Red,
}

pub struct CascadeContext<'a> {
    /// Traversal ctx
    _ctx: MemTraversalContext<'a>,

    /// Cascade data structure
    dast: CascadeDast,

    /// The allowed deficit
    /// TODO: should this be measured by work units?
    _deficit_parameter: i64,

    /// Cached result of cascade voting
    cached_vote: bool,
}

impl<'a> CascadeContext<'a> {
    pub fn new(_ctx: MemTraversalContext<'a>, _deficit_parameter: i64) -> Self {
        let cached_vote = true; // The empty set is a d-UMC by definition
        Self { _ctx, dast: Default::default(), _deficit_parameter, cached_vote }
    }

    /// Insert a new block `hash` where `blue` indicates whether the block is blue or not.
    /// Returns whether the resulting blue cluster *contains* a subset of blocks which is
    /// a d-UMC (via incremental cascade voting)
    pub fn insert(&mut self, hash: Hash, colouring: BlockColouring) -> bool {
        self.dast.g.insert(hash).then_some(()).unwrap();
        if let BlockColouring::Blue { anticone_blues, past } = colouring {
            self.dast.blueset.insert(hash).then_some(()).unwrap();

            let total_blues = self.dast.blueset.len() as u64;
            let total_reds = self.dast.g.len() as u64 - total_blues;
            let past_blues = total_blues - 1 - anticone_blues; // -1 for this block; future is empty
            let past_reds = past - past_blues;
            let anticone_reds = total_reds - past_reds; // this block is not red, so there is no need to subtract 1; future is empty

            self.dast.tree.insert(hash, past_blues, past_reds, anticone_blues, anticone_reds).then_some(()).unwrap();

            if self.cached_vote {
                // A blue block preserves the positive vote
                return true;
            }
        } else if !self.cached_vote {
            // A red block preserves the negative votes
            return true;
        }

        self.cached_vote = self.vote();
        self.cached_vote
    }

    // fn peek_min(&self) -> CascadeTreeEntry {
    //     self.dast.tree.peek_min()
    // }

    pub fn vote(&mut self) -> bool {
        todo!()
    }
}

#[derive(Clone)]
pub struct DagPlan {
    genesis: u64,
    blocks: Vec<(u64, Vec<u64>)>, // All blocks other than genesis
}

impl DagPlan {
    /// Returns all block ids other than genesis
    pub fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.blocks.iter().map(|(i, _)| *i)
    }

    pub fn genesis(&self) -> u64 {
        self.genesis
    }
}

#[derive(PartialEq, Eq)]
pub struct RankValue {
    pub k: KType,
    pub selected_parent: SortableBlock,
}

impl PartialOrd for RankValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankValue {
    /// Sample ordering:
    /// { k: 0, sp_bw: 1} < { k: 1, sp_bw: 1}   => one "k" is lower than another
    /// { k: 0, sp_bw: 10} < { k: 0, sp_bw: 1}  => same "k", different blue work. rankvalue with higher bw comes first
    /// { k: 1, sp_bw: 5, sp_hash: 77} < { k: 1, sp_bw: 5, sp_hash: 66} => same "k" and "bw", rankvalue with higher sp hash value comes first
    fn cmp(&self, other: &Self) -> Ordering {
        if self.k == other.k {
            // let ordering = self.selected_parent.cmp(&other.selected_parent);
            // NOTE: When ordering by RankValue and k is the same, a "smaller" rank would mean a "greater" selected parent
            let ordering = other.selected_parent.cmp(&self.selected_parent);
            // println!("a: {} | b: {} | ordering: {:?}", self.selected_parent.blue_work, other.selected_parent.blue_work, ordering);
            return ordering;
        }

        self.k.cmp(&other.k)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::{cell::RefCell, fs::File};

    use kaspa_consensus_core::blockhash::ORIGIN;
    use kaspa_consensus_core::header::Header;
    use parking_lot::lock_api::RwLock;

    use super::*;
    use crate::model::stores::ghostdag::GhostdagStore;
    use crate::model::stores::headers::MemoryHeaderStore;
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
        let coloring_gd_manager = GhostdagManager::new(
            genesis_hash,
            k_max,
            coloring_ghostdag_store.clone(),
            relations.clone(),
            headers_store.clone(),
            reachability.clone(),
        );

        coloring_ghostdag_store.insert(genesis_hash, Arc::new(coloring_gd_manager.genesis_ghostdag_data())).unwrap();

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

        let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));

        let dk_executor = DagknightExecutor {
            genesis_hash,
            dagknight_store: dagknight_store.clone(),
            ghostdag_store: topology_ghostdag_store.clone(),
            headers_store: headers_store.clone(),
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability.clone()))),
            relations_store: Arc::new(RwLock::new(relations.clone())),
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

            let selected_parent = dk_executor.dagknight(&new_block.parents);

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
        let selected_parent = dk_executor.dagknight(&virtual_block.parents.clone());
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
            ghostdag_store: topology_ghostdag_store.clone(),
            headers_store: headers_store.clone(),
            reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability.clone()))),
            relations_store: Arc::new(RwLock::new(relations.clone())),
        };
        let mut builder = DagBuilder::new(&mut reachability, &mut relations);
        builder.init();
        let genesis = DagBlock::new(genesis_hash, vec![ORIGIN]);
        builder.add_block(genesis.clone());

        // Add blocks 2 and 3 and insert headers/ghostdag entries.
        // We'll use a small helper closure to reduce repetition when adding a block and its header.
        let mut add_block_with_header = |id: u64, parents: Vec<Hash>| {
            let current_hash = id.into();
            let selected_parent = dk_executor.dagknight(&parents);
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

        let virtual_sp = dk_executor.dagknight(&[hash_of_2, hash_of_3]);
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

        let new_sp_virtual = dk_executor.dagknight(&tips);
        println!("new virtual sp: {}", new_sp_virtual);

        assert!(
            reachability.is_chain_ancestor_of(virtual_sp, new_sp_virtual),
            "The selected parent chain changed after attacker raised the rank of previously selected tip"
        )
    }
}
