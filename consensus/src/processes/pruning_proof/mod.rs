use std::{
    cmp::{max, Reverse},
    collections::BinaryHeap,
    sync::Arc,
};

use consensus_core::{
    blockhash::{BlockHashes, ORIGIN},
    header::Header,
    pruning::PruningPointProof,
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher,
};
use itertools::Itertools;
use kaspa_core::trace;
use parking_lot::RwLock;
use rocksdb::WriteBatch;

use crate::{
    consensus::DbGhostdagManager,
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, MemoryRelationsStore, RelationsStore},
            DB,
        },
    },
    processes::ghostdag::ordering::SortableBlock,
};
use std::collections::hash_map::Entry::Vacant;

use super::{ghostdag::protocol::GhostdagManager, parents_builder::ParentsManager, reachability};

pub struct PruningProofManager {
    db: Arc<DB>,
    headers_store: Arc<DbHeadersStore>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, DbRelationsStore>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    relations_stores: Vec<Arc<RwLock<DbRelationsStore>>>,
    ghostdag_managers: Vec<DbGhostdagManager>,

    max_block_level: BlockLevel,
}

struct HeaderStoreMock {}

#[allow(unused_variables)]
impl HeaderStoreReader for HeaderStoreMock {
    fn get_daa_score(&self, hash: hashes::Hash) -> Result<u64, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_blue_score(&self, hash: hashes::Hash) -> Result<u64, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_timestamp(&self, hash: hashes::Hash) -> Result<u64, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_bits(&self, hash: hashes::Hash) -> Result<u32, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_header(&self, hash: hashes::Hash) -> Result<Arc<Header>, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_header_with_block_level(
        &self,
        hash: hashes::Hash,
    ) -> Result<crate::model::stores::headers::HeaderWithBlockLevel, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_compact_header_data(
        &self,
        hash: hashes::Hash,
    ) -> Result<crate::model::stores::headers::CompactHeaderData, crate::model::stores::errors::StoreError> {
        todo!()
    }
}

struct GhostdagStoreMock {}

#[allow(unused_variables)]
impl GhostdagStoreReader for GhostdagStoreMock {
    fn get_blue_score(&self, hash: hashes::Hash) -> Result<u64, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_blue_work(&self, hash: hashes::Hash) -> Result<consensus_core::BlueWorkType, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_selected_parent(&self, hash: hashes::Hash) -> Result<hashes::Hash, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_mergeset_blues(&self, hash: hashes::Hash) -> Result<BlockHashes, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_mergeset_reds(&self, hash: hashes::Hash) -> Result<BlockHashes, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_blues_anticone_sizes(
        &self,
        hash: hashes::Hash,
    ) -> Result<crate::model::stores::ghostdag::HashKTypeMap, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_data(
        &self,
        hash: hashes::Hash,
    ) -> Result<Arc<crate::model::stores::ghostdag::GhostdagData>, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn get_compact_data(
        &self,
        hash: hashes::Hash,
    ) -> Result<crate::model::stores::ghostdag::CompactGhostdagData, crate::model::stores::errors::StoreError> {
        todo!()
    }

    fn has(&self, hash: hashes::Hash) -> Result<bool, crate::model::stores::errors::StoreError> {
        todo!()
    }
}

impl PruningProofManager {
    pub fn apply_proof(&self, proof: PruningPointProof) {
        self.populate_reachability(&proof);
        for (level, headers) in proof.iter().enumerate() {
            trace!("Applying level {} in pruning point proof", level);
            for header in headers {
                let parents = self
                    .parents_manager
                    .parents_at_level(header, level as BlockLevel)
                    .iter()
                    .copied()
                    .filter(|parent| self.ghostdag_stores[level].has(*parent).unwrap())
                    .collect_vec();

                let parents = Arc::new(if parents.is_empty() {
                    assert_ne!(level, 0, "the header is expected to have at least one direct parent");
                    vec![ORIGIN]
                } else {
                    parents
                });

                self.relations_stores[level].write().insert(header.hash, parents.clone()).unwrap();
                let mut gd = self.ghostdag_managers[level].ghostdag(&parents);
                if level == 0 {
                    // Override the ghostdag data with the real blue score and blue work
                    gd = Arc::new(GhostdagData {
                        blue_score: header.blue_score,
                        blue_work: header.blue_work,
                        selected_parent: gd.selected_parent,
                        mergeset_blues: gd.mergeset_blues.clone(),
                        mergeset_reds: gd.mergeset_reds.clone(),
                        blues_anticone_sizes: gd.blues_anticone_sizes.clone(),
                    });
                }
                self.ghostdag_stores[level].insert(header.hash, gd).unwrap();
            }
        }
    }

    pub fn populate_reachability(&self, proof: &PruningPointProof) {
        let mut dag = BlockHashMap::new(); // TODO: Consider making a capacity estimation here
        let mut up_heap = BinaryHeap::new();
        for header in proof.iter().cloned().flatten() {
            if let Vacant(e) = dag.entry(header.hash) {
                let state = pow::State::new(&header);
                let (_, pow) = state.check_pow(header.nonce); // TODO: Check if pow passes
                let signed_block_level = self.max_block_level as i64 - pow.bits() as i64;
                let block_level = max(signed_block_level, 0) as BlockLevel;
                self.headers_store.insert(header.hash, header.clone(), block_level).unwrap();

                let mut parents = BlockHashSet::new(); // TODO: Consider making a capacity estimation here
                for level in 0..=self.max_block_level {
                    for parent in self.parents_manager.parents_at_level(&header, level) {
                        parents.insert(*parent);
                    }
                }

                struct DagEntry {
                    header: Arc<Header>,
                    parents: Arc<BlockHashSet>,
                }

                up_heap.push(Reverse(SortableBlock { hash: header.hash, blue_work: header.blue_work }));
                e.insert(DagEntry { header, parents: Arc::new(parents) });
            }
        }

        let relations_store = Arc::new(RwLock::new(MemoryRelationsStore::new()));
        let relations_service = Arc::new(MTRelationsService::new(relations_store.clone()));
        let gm = GhostdagManager::new(
            0.into(),
            0,
            Arc::new(GhostdagStoreMock {}),
            relations_service,
            Arc::new(HeaderStoreMock {}),
            self.reachability_service.clone(),
        ); // Nothing except reachability and relations should be used, so all other arguments can be fake.

        let mut selected_tip = up_heap.peek().unwrap().clone().0;
        for reverse_sortable_block in up_heap {
            let hash = reverse_sortable_block.0.hash;
            let dag_entry = dag.get(&hash).unwrap();
            let parents_in_dag = BinaryHeap::from_iter(
                dag_entry
                    .parents
                    .iter()
                    .cloned()
                    .filter(|parent| dag.contains_key(parent))
                    .map(|parent| SortableBlock { hash: parent, blue_work: dag.get(&parent).unwrap().header.blue_work }),
            );

            let mut fake_direct_parents: Vec<SortableBlock> = Vec::new();
            for parent in parents_in_dag {
                if self
                    .reachability_service
                    .is_dag_ancestor_of_any(parent.hash, &mut fake_direct_parents.iter().map(|parent| &parent.hash).cloned())
                {
                    continue;
                }

                fake_direct_parents.push(parent);
            }

            let fake_direct_parents_hashes = BlockHashes::new(if fake_direct_parents.is_empty() {
                vec![ORIGIN]
            } else {
                fake_direct_parents.iter().map(|parent| &parent.hash).cloned().collect_vec()
            });

            let selected_parent = fake_direct_parents.iter().max().map(|parent| parent.hash).unwrap_or(ORIGIN);

            relations_store.write().insert(hash, fake_direct_parents_hashes.clone()).unwrap();
            let mergeset = gm.ordered_mergeset_without_selected_parent(selected_parent, &fake_direct_parents_hashes);
            let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());
            reachability::inquirer::add_block(&mut staging, hash, selected_parent, &mut mergeset.iter().cloned()).unwrap();
            let reachability_write_guard = staging.commit(&mut WriteBatch::default()).unwrap();
            drop(reachability_write_guard);

            selected_tip = max(selected_tip, reverse_sortable_block.0);
        }
    }
}
