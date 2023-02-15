use std::{
    cmp::{max, Reverse},
    collections::BinaryHeap,
    sync::Arc,
};

use consensus_core::{
    blockhash::{BlockHashes, ORIGIN},
    ghostdag::TrustedBlock,
    header::Header,
    pruning::PruningPointProof,
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher,
};
use database::prelude::StoreError;
use hashes::Hash;
use itertools::Itertools;
use kaspa_core::{info, trace};
use parking_lot::RwLock;
use rocksdb::WriteBatch;

use crate::{
    consensus::{DbGhostdagManager, VirtualStores},
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::{DbHeadersSelectedTipStore, HeadersSelectedTipStore},
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore},
            pruning::{DbPruningStore, PruningStore},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, MemoryRelationsStore, RelationsStore},
            tips::DbTipsStore,
            virtual_state::{VirtualState, VirtualStateStore},
            DB,
        },
    },
    processes::ghostdag::ordering::SortableBlock,
};
use std::collections::hash_map::Entry::Vacant;

use super::{ghostdag::protocol::GhostdagManager, parents_builder::ParentsManager, reachability};
use kaspa_utils::binary_heap::BinaryHeapExtensions;

pub struct PruningProofManager {
    db: Arc<DB>,
    headers_store: Arc<DbHeadersStore>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, DbRelationsStore>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    pruning_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    virtual_stores: Arc<RwLock<VirtualStores>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    depth_store: Arc<DbDepthStore>,

    ghostdag_managers: Vec<DbGhostdagManager>,

    max_block_level: BlockLevel,
    genesis_hash: Hash,
}

struct HeaderStoreMock {}

#[allow(unused_variables)]
impl HeaderStoreReader for HeaderStoreMock {
    fn get_daa_score(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
        todo!()
    }

    fn get_blue_score(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
        todo!()
    }

    fn get_timestamp(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
        todo!()
    }

    fn get_bits(&self, hash: hashes::Hash) -> Result<u32, StoreError> {
        todo!()
    }

    fn get_header(&self, hash: hashes::Hash) -> Result<Arc<Header>, StoreError> {
        todo!()
    }

    fn get_header_with_block_level(
        &self,
        hash: hashes::Hash,
    ) -> Result<crate::model::stores::headers::HeaderWithBlockLevel, StoreError> {
        todo!()
    }

    fn get_compact_header_data(&self, hash: hashes::Hash) -> Result<crate::model::stores::headers::CompactHeaderData, StoreError> {
        todo!()
    }
}

struct GhostdagStoreMock {}

#[allow(unused_variables)]
impl GhostdagStoreReader for GhostdagStoreMock {
    fn get_blue_score(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
        todo!()
    }

    fn get_blue_work(&self, hash: hashes::Hash) -> Result<consensus_core::BlueWorkType, StoreError> {
        todo!()
    }

    fn get_selected_parent(&self, hash: hashes::Hash) -> Result<hashes::Hash, StoreError> {
        todo!()
    }

    fn get_mergeset_blues(&self, hash: hashes::Hash) -> Result<BlockHashes, StoreError> {
        todo!()
    }

    fn get_mergeset_reds(&self, hash: hashes::Hash) -> Result<BlockHashes, StoreError> {
        todo!()
    }

    fn get_blues_anticone_sizes(&self, hash: hashes::Hash) -> Result<crate::model::stores::ghostdag::HashKTypeMap, StoreError> {
        todo!()
    }

    fn get_data(&self, hash: hashes::Hash) -> Result<Arc<crate::model::stores::ghostdag::GhostdagData>, StoreError> {
        todo!()
    }

    fn get_compact_data(&self, hash: hashes::Hash) -> Result<crate::model::stores::ghostdag::CompactGhostdagData, StoreError> {
        todo!()
    }

    fn has(&self, hash: hashes::Hash) -> Result<bool, StoreError> {
        todo!()
    }
}

#[allow(clippy::too_many_arguments)]
impl PruningProofManager {
    pub fn new(
        db: Arc<DB>,
        headers_store: Arc<DbHeadersStore>,
        reachability_store: Arc<RwLock<DbReachabilityStore>>,
        parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, DbRelationsStore>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
        relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        virtual_stores: Arc<RwLock<VirtualStores>>,
        body_tips_store: Arc<RwLock<DbTipsStore>>,
        headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
        depth_store: Arc<DbDepthStore>,
        ghostdag_managers: Vec<DbGhostdagManager>,
        max_block_level: BlockLevel,
        genesis_hash: Hash,
    ) -> Self {
        Self {
            db,
            headers_store,
            reachability_store,
            parents_manager,
            reachability_service,
            ghostdag_stores,
            relations_stores,
            pruning_store,
            past_pruning_points_store,
            virtual_stores,
            body_tips_store,
            headers_selected_tip_store,
            depth_store,
            ghostdag_managers,
            max_block_level,
            genesis_hash,
        }
    }

    pub fn import_pruning_points(&self, pruning_points: &[Arc<Header>]) {
        // TODO: Also write validate_pruning_points
        for (i, header) in pruning_points.iter().enumerate() {
            self.past_pruning_points_store.insert(i as u64, header.hash).unwrap();

            if self.headers_store.has(header.hash).unwrap() {
                continue;
            }

            let state = pow::State::new(header);
            let (_, pow) = state.check_pow(header.nonce);
            let signed_block_level = self.max_block_level as i64 - pow.bits() as i64;
            let block_level = max(signed_block_level, 0) as BlockLevel;
            self.headers_store.insert(header.hash, header.clone(), block_level).unwrap();
        }
        let current_pp = pruning_points.last().unwrap().hash;
        info!("Setting {current_pp} as the current pruning point");
        self.pruning_store.write().set(current_pp, current_pp, (pruning_points.len() - 1) as u64).unwrap();
    }

    pub fn apply_proof(&self, mut proof: PruningPointProof, trusted_set: &[TrustedBlock]) {
        let proof_zero_set = BlockHashSet::from_iter(proof[0].iter().map(|header| header.hash));
        let mut trusted_gd_map: BlockHashMap<GhostdagData> = BlockHashMap::new();
        for tb in trusted_set.iter() {
            trusted_gd_map.insert(tb.block.hash(), tb.ghostdag.clone().into());
            if proof_zero_set.contains(&tb.block.hash()) {
                continue;
            }

            proof[0].push(tb.block.header.clone());
        }

        proof[0].sort_by(|a, b| a.blue_work.cmp(&b.blue_work));
        self.populate_reachability(&proof);
        for (level, headers) in proof.iter().enumerate() {
            trace!("Applying level {} in pruning point proof", level);
            self.ghostdag_stores[level].insert(ORIGIN, self.ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            for header in headers.iter() {
                let parents = self
                    .parents_manager
                    .parents_at_level(header, level as BlockLevel)
                    .iter()
                    .copied()
                    .filter(|parent| self.ghostdag_stores[level].has(*parent).unwrap())
                    .collect_vec();

                let parents = Arc::new(if parents.is_empty() { vec![ORIGIN] } else { parents });

                self.relations_stores.write()[level].insert(header.hash, parents.clone()).unwrap();
                let gd = if header.hash == self.genesis_hash {
                    self.ghostdag_managers[level].genesis_ghostdag_data()
                } else if level == 0 {
                    if let Some(gd) = trusted_gd_map.get(&header.hash) {
                        gd.clone()
                    } else {
                        let calculated_gd = self.ghostdag_managers[level].ghostdag(&parents);
                        // Override the ghostdag data with the real blue score and blue work
                        GhostdagData {
                            blue_score: header.blue_score,
                            blue_work: header.blue_work,
                            selected_parent: calculated_gd.selected_parent,
                            mergeset_blues: calculated_gd.mergeset_blues.clone(),
                            mergeset_reds: calculated_gd.mergeset_reds.clone(),
                            blues_anticone_sizes: calculated_gd.blues_anticone_sizes.clone(),
                        }
                    }
                } else {
                    self.ghostdag_managers[level].ghostdag(&parents)
                };
                self.ghostdag_stores[level].insert(header.hash, Arc::new(gd)).unwrap();
            }
        }

        let pruning_point_header = proof[0].last().unwrap();
        let pruning_point = pruning_point_header.hash;
        let virtual_parents = vec![pruning_point];
        let virtual_gd = self.ghostdag_managers[0].ghostdag(&virtual_parents);

        let virtual_state = VirtualState {
            // TODO: Use real values when possible
            parents: virtual_parents.clone(),
            ghostdag_data: virtual_gd,
            daa_score: 0,
            bits: 0,
            multiset: Default::default(),
            utxo_diff: Default::default(),
            accepted_tx_ids: vec![],
            mergeset_rewards: Default::default(),
            mergeset_non_daa: Default::default(),
            past_median_time: 0,
        };
        self.virtual_stores.write().state.set(virtual_state).unwrap();

        let mut batch = WriteBatch::default();
        self.body_tips_store.write().init_batch(&mut batch, &virtual_parents).unwrap();
        self.headers_selected_tip_store
            .write()
            .set(SortableBlock { hash: pruning_point, blue_work: pruning_point_header.blue_work })
            .unwrap();
        // self.depth_store.insert_batch(&mut batch, pruning_point, pruning_point, pruning_point).unwrap();
        self.db.write(batch).unwrap();
    }

    pub fn populate_reachability(&self, proof: &PruningPointProof) {
        let mut dag = BlockHashMap::new(); // TODO: Consider making a capacity estimation here
        let mut up_heap = BinaryHeap::new();
        for header in proof.iter().flatten().cloned() {
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

        let relations_store = Arc::new(RwLock::new(vec![MemoryRelationsStore::new()]));
        relations_store.write()[0].insert(ORIGIN, Arc::new(vec![])).unwrap();
        let relations_service = MTRelationsService::new(relations_store.clone(), 0);
        let gm = GhostdagManager::new(
            0.into(),
            0,
            Arc::new(GhostdagStoreMock {}),
            relations_service,
            Arc::new(HeaderStoreMock {}),
            self.reachability_service.clone(),
        ); // Nothing except reachability and relations should be used, so all other arguments can be fake.

        let mut selected_tip = up_heap.peek().unwrap().clone().0;
        for reverse_sortable_block in up_heap.into_sorted_iter() {
            // TODO: Convert to into_iter_sorted once it gets stable
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
            for parent in parents_in_dag.into_sorted_iter() {
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

            relations_store.write()[0].insert(hash, fake_direct_parents_hashes.clone()).unwrap();
            let mergeset = gm.unordered_mergeset_without_selected_parent(selected_parent, &fake_direct_parents_hashes);
            let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());
            reachability::inquirer::add_block(&mut staging, hash, selected_parent, &mut mergeset.iter().cloned()).unwrap();
            let reachability_write_guard = staging.commit(&mut WriteBatch::default()).unwrap();
            drop(reachability_write_guard);

            selected_tip = max(selected_tip, reverse_sortable_block.0);
        }
    }
}
