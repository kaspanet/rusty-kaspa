use std::sync::Arc;

use kaspa_consensus_core::BlockHashMap;
use kaspa_hashes::Hash;
use parking_lot::RwLock;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{headers::DbHeadersStore, reachability::DbReachabilityStore, relations::DbRelationsStore},
};

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
                3. Representatives
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
