use std::sync::Arc;

use kaspa_consensus_core::{BlockHashMap, BlockHashSet};
use kaspa_hashes::Hash;
use parking_lot::RwLock;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        children::ChildrenStore,
        headers::DbHeadersStore,
        reachability::{DbReachabilityStore, MemoryReachabilityStore, ReachabilityStore},
        relations::{DbRelationsStore, MemoryRelationsStore, RelationsStore},
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
*/

pub struct DagknightConflictEntry {
    // TODO: incremental colouring data for relevant k values
}

pub struct DagknightData {
    /// A mapping from conflict roots to incremental conflict data
    entries: BlockHashMap<DagknightConflictEntry>,

    /// The selected parent of this block as chosen by the DAGKNIGHT protocol
    selected_parent: Hash,
}

/// A struct encapsulating the logic and algorithms of the DAGKNIGHT protocol
pub struct DagknightExecutor {
    // TODO: access to relevant stores and to the reachability service

    // pub(super) k: KType,
    genesis_hash: Hash,
    pub(super) relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
}

impl DagknightExecutor {
    pub fn dagknight(&self, _parents: &[Hash]) -> DagknightData {
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

        todo!()
    }

    fn common_chain_ancestor(&self, parents: &[Hash]) -> Hash {
        /*
           Notes:
               - ignore parents not agreeing on the pruning point as a chain block
               - optimize for shortest path
               - optimize with index
        */

        let start = parents[0];
        for cb in self.reachability_service.default_backward_chain_iterator(start).skip(1) {
            if self.reachability_service.is_chain_ancestor_of_all(cb, &parents[1..]) {
                return cb;
            }
        }

        panic!("")
    }

    fn umc_cascade_voting(&self) {
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
}

mod ct {
    use super::*;
    use std::{
        cmp::Ordering,
        collections::{
            hash_map::Entry::{Occupied, Vacant},
            BTreeSet,
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
        pub fn update_anticone_blues(&mut self, hash: Hash, anticone_blues: u64) {
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
        pub fn update_anticone_reds_lower_bound(&mut self, hash: Hash, anticone_reds_lower_bound: u64) {
            let prev_floor = self.rev_index[&hash];
            let prev_arlb = self.arlb.insert(hash, anticone_reds_lower_bound).unwrap();
            let new_floor = prev_floor + (anticone_reds_lower_bound as i64 - prev_arlb as i64);
            self.btree.remove(&CascadeTreeEntry::new(hash, prev_floor)).then_some(()).unwrap();
            self.btree.insert(CascadeTreeEntry::new(hash, new_floor)).then_some(()).unwrap();
            self.rev_index.insert(hash, new_floor);
            assert!(anticone_reds_lower_bound > prev_arlb);
        }

        pub fn peek_min(&self) -> CascadeTreeEntry {
            self.btree.first().cloned().unwrap()
        }
    }
}

use ct::{CascadeTree, CascadeTreeEntry};

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
    oracle: &'a T,
    /// The relations oracle (local DAG area)
    relations: &'a S,
}

impl<'a, T: ReachabilityStore + ?Sized, S: RelationsStore + ChildrenStore + ?Sized> TraversalContext<'a, T, S> {
    pub fn new(reachability: &'a T, relations: &'a S) -> Self {
        Self { oracle: reachability, relations }
    }
}

pub type MemTraversalContext<'a> = TraversalContext<'a, MemoryReachabilityStore, MemoryRelationsStore>;

pub enum BlockColouring {
    Blue { anticone_blues: u64, past: u64 },
    Red,
}

pub struct CascadeContext<'a> {
    /// Traversal ctx
    ctx: MemTraversalContext<'a>,

    /// Cascade data structure
    dast: CascadeDast,

    /// The allowed deficit
    /// TODO: should this be measured by work units?
    deficit_parameter: i64,

    /// Cached result of cascade voting
    cached_vote: bool,
}

impl<'a> CascadeContext<'a> {
    pub fn new(ctx: MemTraversalContext<'a>, deficit_parameter: i64) -> Self {
        let cached_vote = true; // The empty set is a d-UMC by definition
        Self { ctx, dast: Default::default(), deficit_parameter, cached_vote }
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

    fn peek_min(&self) -> CascadeTreeEntry {
        self.dast.tree.peek_min()
    }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::stores::{reachability::MemoryReachabilityStore, relations::MemoryRelationsStore},
        processes::reachability::tests::{DagBlock, DagBuilder},
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
}
