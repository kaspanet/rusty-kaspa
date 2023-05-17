use std::{
    cmp::{max, Reverse},
    collections::hash_map::Entry::Vacant,
    collections::BinaryHeap,
    ops::DerefMut,
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    errors::{
        consensus::{ConsensusError, ConsensusResult},
        pruning::{PruningImportError, PruningImportResult},
    },
    header::Header,
    pruning::{PruningPointProof, PruningPointTrustedData},
    trusted::{TrustedBlock, TrustedGhostdagData, TrustedHeader},
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
};
use kaspa_core::{info, trace};
use kaspa_database::prelude::{StoreError, StoreResultEmptyTuple, StoreResultExtensions};
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use kaspa_utils::binary_heap::BinaryHeapExtensions;
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
            block_window_cache::{BlockWindowCacheStore, BlockWindowHeap},
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::DbHeadersSelectedTipStore,
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore},
            pruning::{DbPruningStore, PruningStore, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::{DbRelationsStore, MemoryRelationsStore, RelationsStore, RelationsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            tips::DbTipsStore,
            virtual_state::{VirtualState, VirtualStateStore, VirtualStateStoreReader},
            DB,
        },
    },
    processes::{
        ghostdag::{ordering::SortableBlock, protocol::GhostdagManager},
        parents_builder::ParentsManager,
        reachability::inquirer as reachability,
        traversal_manager::DagTraversalManager,
        window::{FullWindowManager, WindowManager, WindowType},
    },
};

struct CachedPruningPointData<T: ?Sized> {
    pruning_point: Hash,
    data: Arc<T>,
}

impl<T> Clone for CachedPruningPointData<T> {
    fn clone(&self) -> Self {
        Self { pruning_point: self.pruning_point, data: self.data.clone() }
    }
}

pub struct PruningProofManager {
    db: Arc<DB>,
    headers_store: Arc<DbHeadersStore>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    pruning_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    virtual_stores: Arc<RwLock<VirtualStores>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    depth_store: Arc<DbDepthStore>,
    selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,

    ghostdag_managers: Vec<DbGhostdagManager>,
    traversal_manager: DagTraversalManager<DbGhostdagStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    window_manager: FullWindowManager<DbGhostdagStore, BlockWindowCacheStore, DbHeadersStore>,

    cached_proof: RwLock<Option<CachedPruningPointData<PruningPointProof>>>,
    cached_anticone: RwLock<Option<CachedPruningPointData<PruningPointTrustedData>>>,

    max_block_level: BlockLevel,
    genesis_hash: Hash,
    pruning_proof_m: u64,
    anticone_finalization_depth: u64,
    ghostdag_k: KType,
}

struct HeaderStoreMock {}

#[allow(unused_variables)]
impl HeaderStoreReader for HeaderStoreMock {
    fn get_daa_score(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
        unimplemented!()
    }

    fn get_blue_score(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
        unimplemented!()
    }

    fn get_timestamp(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
        unimplemented!()
    }

    fn get_bits(&self, hash: kaspa_hashes::Hash) -> Result<u32, StoreError> {
        unimplemented!()
    }

    fn get_header(&self, hash: kaspa_hashes::Hash) -> Result<Arc<Header>, StoreError> {
        unimplemented!()
    }

    fn get_header_with_block_level(
        &self,
        hash: kaspa_hashes::Hash,
    ) -> Result<crate::model::stores::headers::HeaderWithBlockLevel, StoreError> {
        unimplemented!()
    }

    fn get_compact_header_data(
        &self,
        hash: kaspa_hashes::Hash,
    ) -> Result<crate::model::stores::headers::CompactHeaderData, StoreError> {
        unimplemented!()
    }
}

struct GhostdagStoreMock {}

#[allow(unused_variables)]
impl GhostdagStoreReader for GhostdagStoreMock {
    fn get_blue_score(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
        unimplemented!()
    }

    fn get_blue_work(&self, hash: kaspa_hashes::Hash) -> Result<kaspa_consensus_core::BlueWorkType, StoreError> {
        unimplemented!()
    }

    fn get_selected_parent(&self, hash: kaspa_hashes::Hash) -> Result<kaspa_hashes::Hash, StoreError> {
        unimplemented!()
    }

    fn get_mergeset_blues(&self, hash: kaspa_hashes::Hash) -> Result<BlockHashes, StoreError> {
        unimplemented!()
    }

    fn get_mergeset_reds(&self, hash: kaspa_hashes::Hash) -> Result<BlockHashes, StoreError> {
        unimplemented!()
    }

    fn get_blues_anticone_sizes(&self, hash: kaspa_hashes::Hash) -> Result<crate::model::stores::ghostdag::HashKTypeMap, StoreError> {
        unimplemented!()
    }

    fn get_data(&self, hash: kaspa_hashes::Hash) -> Result<Arc<crate::model::stores::ghostdag::GhostdagData>, StoreError> {
        unimplemented!()
    }

    fn get_compact_data(&self, hash: kaspa_hashes::Hash) -> Result<crate::model::stores::ghostdag::CompactGhostdagData, StoreError> {
        unimplemented!()
    }

    fn has(&self, hash: kaspa_hashes::Hash) -> Result<bool, StoreError> {
        unimplemented!()
    }
}

#[allow(clippy::too_many_arguments)]
impl PruningProofManager {
    pub fn new(
        db: Arc<DB>,
        headers_store: Arc<DbHeadersStore>,
        reachability_store: Arc<RwLock<DbReachabilityStore>>,
        parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
        relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        virtual_stores: Arc<RwLock<VirtualStores>>,
        body_tips_store: Arc<RwLock<DbTipsStore>>,
        headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
        depth_store: Arc<DbDepthStore>,
        selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
        ghostdag_managers: Vec<DbGhostdagManager>,
        traversal_manager: DagTraversalManager<DbGhostdagStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
        window_manager: FullWindowManager<DbGhostdagStore, BlockWindowCacheStore, DbHeadersStore>,
        max_block_level: BlockLevel,
        genesis_hash: Hash,
        pruning_proof_m: u64,
        anticone_finalization_depth: u64,
        ghostdag_k: KType,
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
            selected_chain_store,
            depth_store,
            ghostdag_managers,
            traversal_manager,
            window_manager,

            cached_proof: RwLock::new(None),
            cached_anticone: RwLock::new(None),

            max_block_level,
            genesis_hash,
            pruning_proof_m,
            anticone_finalization_depth,
            ghostdag_k,
        }
    }

    pub fn import_pruning_points(&self, pruning_points: &[Arc<Header>]) {
        // TODO: Also write validate_pruning_points
        for (i, header) in pruning_points.iter().enumerate() {
            self.past_pruning_points_store.set(i as u64, header.hash).unwrap();

            if self.headers_store.has(header.hash).unwrap() {
                continue;
            }

            let state = kaspa_pow::State::new(header);
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
        let pruning_point_header = proof[0].last().unwrap().clone();
        let pruning_point = pruning_point_header.hash;

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

        let virtual_parents = vec![pruning_point];
        let virtual_state = Arc::new(VirtualState {
            parents: virtual_parents.clone(),
            ghostdag_data: self.ghostdag_managers[0].ghostdag(&virtual_parents),
            ..VirtualState::default()
        });
        self.virtual_stores.write().state.set(virtual_state).unwrap();

        let mut batch = WriteBatch::default();
        self.body_tips_store.write().init_batch(&mut batch, &virtual_parents).unwrap();
        self.headers_selected_tip_store
            .write()
            .set_batch(&mut batch, SortableBlock { hash: pruning_point, blue_work: pruning_point_header.blue_work })
            .unwrap();
        self.selected_chain_store.write().init_with_pruning_point(&mut batch, pruning_point).unwrap();
        // self.depth_store.insert_batch(&mut batch, pruning_point, pruning_point, pruning_point).unwrap();
        self.db.write(batch).unwrap();
    }

    pub fn populate_reachability(&self, proof: &PruningPointProof) {
        let mut dag = BlockHashMap::new(); // TODO: Consider making a capacity estimation here
        let mut up_heap = BinaryHeap::new();
        for header in proof.iter().flatten().cloned() {
            if let Vacant(e) = dag.entry(header.hash) {
                let state = kaspa_pow::State::new(&header);
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
            reachability::add_block(&mut staging, hash, selected_parent, &mut mergeset.iter().cloned()).unwrap();
            let reachability_write_guard = staging.commit(&mut WriteBatch::default()).unwrap();
            drop(reachability_write_guard);

            selected_tip = max(selected_tip, reverse_sortable_block.0);
        }
    }

    pub fn validate_pruning_point_proof(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        if proof.len() != self.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(self.max_block_level as usize + 1));
        }

        let proof_pp_header = proof[0].last().expect("checked if empty");
        let proof_pp = proof_pp_header.hash;
        let proof_pp_level = calc_block_level(proof_pp_header, self.max_block_level);

        let (db_lifetime, db) = kaspa_database::utils::create_temp_db();
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), 2 * self.pruning_proof_m)); // TODO: Think about cache size
        let ghostdag_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(DbGhostdagStore::new(db.clone(), level, 2 * self.pruning_proof_m)))
            .collect_vec();
        let mut relations_stores =
            (0..=self.max_block_level).map(|level| DbRelationsStore::new(db.clone(), level, 2 * self.pruning_proof_m)).collect_vec();
        let reachability_stores = (0..=self.max_block_level)
            .map(|level| {
                Arc::new(RwLock::new(DbReachabilityStore::new_with_alternative_prefix_end(
                    db.clone(),
                    2 * self.pruning_proof_m,
                    level,
                )))
            })
            .collect_vec();

        let reachability_services = (0..=self.max_block_level)
            .map(|level| MTReachabilityService::new(reachability_stores[level as usize].clone()))
            .collect_vec();

        let ghostdag_managers = ghostdag_stores
            .iter()
            .cloned()
            .enumerate()
            .map(|(level, ghostdag_store)| {
                GhostdagManager::new(
                    self.genesis_hash,
                    self.ghostdag_k,
                    ghostdag_store,
                    relations_stores[level].clone(),
                    headers_store.clone(),
                    reachability_services[level].clone(),
                )
            })
            .collect_vec();

        {
            let mut batch = WriteBatch::default();
            for level in 0..=self.max_block_level {
                let level = level as usize;
                reachability::init(reachability_stores[level].write().deref_mut()).unwrap();
                relations_stores[level].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap();
                ghostdag_stores[level].insert(ORIGIN, self.ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            }

            db.write(batch).unwrap();
        }

        let mut selected_tip_by_level = vec![None; self.max_block_level as usize + 1];
        for level in (0..=self.max_block_level).rev() {
            info!("Validating level {level} from the pruning point proof");
            let level_idx = level as usize;
            let mut selected_tip = None;
            for (i, header) in proof[level as usize].iter().enumerate() {
                let header_level = calc_block_level(header, self.max_block_level);
                if header_level < level {
                    return Err(PruningImportError::PruningProofWrongBlockLevel(header.hash, header_level, level));
                }

                headers_store.insert(header.hash, header.clone(), header_level).unwrap_or_exists();

                let parents = self
                    .parents_manager
                    .parents_at_level(header, level)
                    .iter()
                    .copied()
                    .filter(|parent| ghostdag_stores[level_idx].has(*parent).unwrap())
                    .collect_vec();

                let parents: BlockHashes = if parents.is_empty() {
                    if i != 0 {
                        return Err(PruningImportError::PruningProofHeaderWithNoKnownParents(header.hash, level));
                    }
                    vec![ORIGIN]
                } else {
                    parents
                }
                .into();

                if relations_stores[level_idx].has(header.hash).unwrap() {
                    return Err(PruningImportError::PruningProofDuplicateHeaderAtLevel(header.hash, level));
                }

                relations_stores[level_idx].insert(header.hash, parents.clone()).unwrap();
                let ghostdag_data = Arc::new(ghostdag_managers[level_idx].ghostdag(&parents));
                ghostdag_stores[level_idx].insert(header.hash, ghostdag_data.clone()).unwrap();
                selected_tip = Some(match selected_tip {
                    Some(tip) => ghostdag_managers[level_idx].find_selected_parent([tip, header.hash]),
                    None => header.hash,
                });

                let mut reachability_mergeset = {
                    let reachability_read = reachability_stores[level_idx].read();
                    ghostdag_data
                        .unordered_mergeset_without_selected_parent()
                        .filter(|hash| reachability_read.has(*hash).unwrap())
                        .collect_vec() // We collect to vector so reachability_read can be released and let `reachability::add_block` use a write lock.
                        .into_iter()
                };
                reachability::add_block(
                    reachability_stores[level_idx].write().deref_mut(),
                    header.hash,
                    ghostdag_data.selected_parent,
                    &mut reachability_mergeset,
                )
                .unwrap();

                if selected_tip.unwrap() == header.hash {
                    reachability::hint_virtual_selected_parent(reachability_stores[level_idx].write().deref_mut(), header.hash)
                        .unwrap();
                }
            }

            if level < self.max_block_level {
                let block_at_depth_m_at_next_level = self.block_at_depth(
                    &*ghostdag_stores[level_idx + 1],
                    selected_tip_by_level[level_idx + 1].unwrap(),
                    self.pruning_proof_m,
                );
                if !relations_stores[level_idx].has(block_at_depth_m_at_next_level).unwrap() {
                    return Err(PruningImportError::PruningProofMissingBlockAtDepthMFromNextLevel(level, level + 1));
                }
            }

            if selected_tip.unwrap() != proof_pp
                && !self.parents_manager.parents_at_level(proof_pp_header, level).contains(&selected_tip.unwrap())
            {
                return Err(PruningImportError::PruningProofMissesBlocksBelowPruningPoint(selected_tip.unwrap(), level));
            }

            selected_tip_by_level[level_idx] = selected_tip;
        }

        let pruning_read = self.pruning_store.read();
        let relations_read = self.relations_stores.read();
        let current_pp = pruning_read.get().unwrap().pruning_point;
        let current_pp_header = headers_store.get_header(current_pp).unwrap();

        for (level_idx, selected_tip) in selected_tip_by_level.into_iter().enumerate() {
            let level = level_idx as BlockLevel;
            let selected_tip = selected_tip.unwrap();
            if level <= proof_pp_level {
                if selected_tip != proof_pp {
                    return Err(PruningImportError::PruningProofSelectedTipIsNotThePruningPoint(selected_tip, level));
                }
            } else if !self.parents_manager.parents_at_level(proof_pp_header, level).contains(&selected_tip) {
                return Err(PruningImportError::PruningProofSelectedTipNotParentOfPruningPoint(selected_tip, level));
            }

            let proof_selected_tip_gd = ghostdag_stores[level_idx].get_compact_data(selected_tip).unwrap();
            if proof_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            let mut proof_current = selected_tip;
            let mut proof_current_gd = proof_selected_tip_gd;
            let common_ancestor_data = loop {
                match self.ghostdag_stores[level_idx].get_compact_data(proof_current).unwrap_option() {
                    Some(current_gd) => {
                        break Some((proof_current_gd, current_gd));
                    }
                    None => {
                        proof_current = proof_current_gd.selected_parent;
                        if proof_current.is_origin() {
                            break None;
                        }
                        proof_current_gd = ghostdag_stores[level_idx].get_compact_data(proof_current).unwrap();
                    }
                };
            };

            if let Some((proof_common_ancestor_gd, common_ancestor_gd)) = common_ancestor_data {
                let selected_tip_blue_work_diff = proof_selected_tip_gd.blue_work - proof_common_ancestor_gd.blue_work;
                for parent in self.parents_manager.parents_at_level(&current_pp_header, level).iter().copied() {
                    let parent_blue_work = self.ghostdag_stores[level_idx].get_blue_work(parent).unwrap();
                    let parent_blue_work_diff = parent_blue_work - common_ancestor_gd.blue_work;
                    if parent_blue_work_diff >= selected_tip_blue_work_diff {
                        return Err(PruningImportError::PruningProofInsufficientBlueWork);
                    }
                }

                return Ok(());
            }
        }

        for level in (0..=self.max_block_level).rev() {
            let level_idx = level as usize;
            match relations_read[level_idx].get_parents(current_pp).unwrap_option() {
                Some(parents) => {
                    if parents
                        .iter()
                        .copied()
                        .any(|parent| self.ghostdag_stores[level_idx].get_blue_score(parent).unwrap() < 2 * self.pruning_proof_m)
                    {
                        return Ok(());
                    }
                }
                None => {
                    // If the current pruning point doesn't have a parent at this level, we consider the proof state to be better.
                    return Ok(());
                }
            }
        }

        drop(pruning_read);
        drop(relations_read);
        drop(db_lifetime);

        Err(PruningImportError::PruningProofNotEnoughHeaders)
    }

    fn build_pruning_point_proof(&self, pp: Hash) -> PruningPointProof {
        if pp == self.genesis_hash {
            return vec![];
        }

        let pp_header = self.headers_store.get_header_with_block_level(pp).unwrap();
        let selected_tip_by_level = (0..=self.max_block_level)
            .map(|level| {
                if level <= pp_header.block_level {
                    pp
                } else {
                    self.ghostdag_managers[level as usize].find_selected_parent(
                        self.parents_manager
                            .parents_at_level(&pp_header.header, level)
                            .iter()
                            .filter(|parent| self.ghostdag_stores[level as usize].has(**parent).unwrap())
                            .cloned(),
                    )
                }
            })
            .collect_vec();

        (0..=self.max_block_level)
            .map(|level| {
                let level = level as usize;
                let selected_tip = selected_tip_by_level[level];
                let block_at_depth_2m = self.block_at_depth(&*self.ghostdag_stores[level], selected_tip, 2 * self.pruning_proof_m);

                let root = if level != self.max_block_level as usize {
                    let block_at_depth_m_at_next_level =
                        self.block_at_depth(&*self.ghostdag_stores[level + 1], selected_tip_by_level[level + 1], self.pruning_proof_m);
                    if self.reachability_service.is_dag_ancestor_of(block_at_depth_m_at_next_level, block_at_depth_2m) {
                        block_at_depth_m_at_next_level
                    } else {
                        self.find_common_ancestor_in_chain_of_a(
                            &*self.ghostdag_stores[level],
                            block_at_depth_m_at_next_level,
                            block_at_depth_2m,
                        )
                    }
                } else {
                    block_at_depth_2m
                };

                let mut headers = Vec::with_capacity(2 * self.pruning_proof_m as usize);
                let mut queue = BlockWindowHeap::new();
                let mut visited = BlockHashSet::new();
                queue.push(Reverse(SortableBlock::new(root, self.ghostdag_stores[level].get_blue_work(root).unwrap())));
                while let Some(current) = queue.pop() {
                    let current = current.0.hash;
                    if !visited.insert(current) {
                        continue;
                    }

                    if !self.reachability_service.is_dag_ancestor_of(current, selected_tip) {
                        continue;
                    }

                    let current_header = self.headers_store.get_header(current).unwrap();
                    headers.push(current_header.clone());
                    for child in self.relations_stores.read()[level].get_children(current).unwrap().iter().copied() {
                        queue.push(Reverse(SortableBlock::new(child, self.ghostdag_stores[level].get_blue_work(child).unwrap())));
                    }
                }

                headers
            })
            .collect_vec()
    }

    fn block_at_depth(&self, ghostdag_store: &impl GhostdagStoreReader, high: Hash, depth: u64) -> Hash {
        let high_gd = ghostdag_store.get_compact_data(high).unwrap();
        let mut current_gd = high_gd;
        let mut current = high;
        while current_gd.blue_score + depth >= high_gd.blue_score {
            if current_gd.selected_parent.is_origin() {
                break;
            }

            current = current_gd.selected_parent;
            current_gd = ghostdag_store.get_compact_data(current).unwrap();
        }
        current
    }

    fn find_common_ancestor_in_chain_of_a(&self, ghostdag_store: &impl GhostdagStoreReader, a: Hash, b: Hash) -> Hash {
        let a_gd = ghostdag_store.get_compact_data(a).unwrap();
        let mut current_gd = a_gd;
        let mut current;
        loop {
            current = current_gd.selected_parent;
            assert!(!current.is_origin());
            if self.reachability_service.is_dag_ancestor_of(current, b) {
                break current;
            }
            current_gd = ghostdag_store.get_compact_data(current).unwrap();
        }
    }

    fn calculate_pruning_point_anticone_and_trusted_data(
        &self,
        pruning_point: Hash,
        virtual_parents: impl Iterator<Item = Hash>,
    ) -> PruningPointTrustedData {
        let anticone = self
            .traversal_manager
            .anticone(pruning_point, virtual_parents, None)
            .expect("no error is expected when max_traversal_allowed is None");
        let mut anticone = self.ghostdag_managers[0].sort_blocks(anticone);
        anticone.insert(0, pruning_point);

        let mut daa_window_blocks = BlockHashMap::new();
        let mut ghostdag_blocks = BlockHashMap::new();

        for anticone_block in anticone.iter().copied() {
            let window = self
                .window_manager
                .block_window(&self.ghostdag_stores[0].get_data(anticone_block).unwrap(), WindowType::FullDifficultyWindow)
                .unwrap();

            for hash in window.into_iter().map(|block| block.0.hash) {
                if daa_window_blocks.contains_key(&hash) {
                    continue;
                }

                daa_window_blocks.insert(
                    hash,
                    TrustedHeader {
                        header: self.headers_store.get_header(hash).unwrap(),
                        ghostdag: (&*self.ghostdag_stores[0].get_data(hash).unwrap()).into(),
                    },
                );
            }

            let mut current = anticone_block;
            for _ in 0..=self.ghostdag_k {
                let current_gd = self.ghostdag_stores[0].get_data(current).unwrap();
                ghostdag_blocks.insert(current, (&*current_gd).into());
                current = current_gd.selected_parent;
            }
        }

        PruningPointTrustedData {
            anticone,
            daa_window_blocks: daa_window_blocks.into_values().collect_vec(),
            ghostdag_blocks: ghostdag_blocks.into_iter().map(|(hash, ghostdag)| TrustedGhostdagData { hash, ghostdag }).collect_vec(),
        }
    }

    pub fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        let pp = self.pruning_store.read().pruning_point().unwrap();
        if let Some(cache) = self.cached_proof.read().clone() {
            if cache.pruning_point == pp {
                return cache.data;
            }
        }
        let proof = Arc::new(self.build_pruning_point_proof(pp));
        self.cached_proof.write().replace(CachedPruningPointData { pruning_point: pp, data: proof.clone() });
        proof
    }

    pub fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        let pp = self.pruning_store.read().pruning_point().unwrap();
        if let Some(cache) = self.cached_anticone.read().clone() {
            if cache.pruning_point == pp {
                return Ok(cache.data);
            }
        }

        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        let pp_bs = self.headers_store.get_blue_score(pp).unwrap();

        // The anticone is considered final only if the pruning point is at sufficient depth from virtual
        if virtual_state.ghostdag_data.blue_score >= pp_bs + self.anticone_finalization_depth {
            let anticone = Arc::new(self.calculate_pruning_point_anticone_and_trusted_data(pp, virtual_state.parents.iter().copied()));
            self.cached_anticone.write().replace(CachedPruningPointData { pruning_point: pp, data: anticone.clone() });
            Ok(anticone)
        } else {
            Err(ConsensusError::PruningPointInsufficientDepth)
        }
    }
}
