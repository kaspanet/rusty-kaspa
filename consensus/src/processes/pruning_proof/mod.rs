use std::{
    cmp::{max, Reverse},
    collections::hash_map::Entry::Vacant,
    collections::BinaryHeap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

use itertools::Itertools;
use kaspa_math::int::SignedInteger;
use parking_lot::{Mutex, RwLock};
use rocksdb::WriteBatch;

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
use kaspa_core::{debug, info, trace};
use kaspa_database::prelude::{ConnBuilder, StoreResultEmptyTuple, StoreResultExtensions};
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use kaspa_utils::{binary_heap::BinaryHeapExtensions, vec::VecExtensions};

use crate::{
    consensus::{
        services::{DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbWindowManager},
        storage::ConsensusStorage,
    },
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::DbHeadersSelectedTipStore,
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore},
            pruning::{DbPruningStore, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStoreReader, StagingRelationsStore},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            tips::DbTipsStore,
            virtual_state::{VirtualState, VirtualStateStore, VirtualStateStoreReader, VirtualStores},
            DB,
        },
    },
    processes::{
        ghostdag::ordering::SortableBlock, reachability::inquirer as reachability, relations::RelationsStoreExtensions,
        window::WindowType,
    },
};

use super::{
    ghostdag::{mergeset::unordered_mergeset_without_selected_parent, protocol::GhostdagManager},
    window::WindowManager,
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
    reachability_relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    ghostdag_stores: Arc<Vec<Arc<DbGhostdagStore>>>,
    relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    pruning_point_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    virtual_stores: Arc<RwLock<VirtualStores>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    depth_store: Arc<DbDepthStore>,
    selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,

    ghostdag_managers: Arc<Vec<DbGhostdagManager>>,
    traversal_manager: DbDagTraversalManager,
    window_manager: DbWindowManager,
    parents_manager: DbParentsManager,

    cached_proof: Mutex<Option<CachedPruningPointData<PruningPointProof>>>,
    cached_anticone: Mutex<Option<CachedPruningPointData<PruningPointTrustedData>>>,

    max_block_level: BlockLevel,
    genesis_hash: Hash,
    pruning_proof_m: u64,
    anticone_finalization_depth: u64,
    ghostdag_k: KType,
}

impl PruningProofManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        parents_manager: DbParentsManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        ghostdag_managers: Arc<Vec<DbGhostdagManager>>,
        traversal_manager: DbDagTraversalManager,
        window_manager: DbWindowManager,
        max_block_level: BlockLevel,
        genesis_hash: Hash,
        pruning_proof_m: u64,
        anticone_finalization_depth: u64,
        ghostdag_k: KType,
    ) -> Self {
        Self {
            db,
            headers_store: storage.headers_store.clone(),
            reachability_store: storage.reachability_store.clone(),
            reachability_relations_store: storage.reachability_relations_store.clone(),
            reachability_service,
            ghostdag_stores: storage.ghostdag_stores.clone(),
            relations_stores: storage.relations_stores.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            headers_selected_tip_store: storage.headers_selected_tip_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            depth_store: storage.depth_store.clone(),

            ghostdag_managers,
            traversal_manager,
            window_manager,
            parents_manager,

            cached_proof: Mutex::new(None),
            cached_anticone: Mutex::new(None),

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

        let new_pruning_point = pruning_points.last().unwrap().hash;
        info!("Setting {new_pruning_point} as the current pruning point");

        let mut pruning_point_write = self.pruning_point_store.write();
        let mut batch = WriteBatch::default();
        pruning_point_write.set_batch(&mut batch, new_pruning_point, new_pruning_point, (pruning_points.len() - 1) as u64).unwrap();
        pruning_point_write.set_history_root(&mut batch, new_pruning_point).unwrap();
        self.db.write(batch).unwrap();
        drop(pruning_point_write);
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
        self.populate_reachability_and_headers(&proof);
        for (level, headers) in proof.iter().enumerate() {
            trace!("Applying level {} from the pruning point proof", level);
            self.ghostdag_stores[level].insert(ORIGIN, self.ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            for header in headers.iter() {
                let parents = Arc::new(
                    self.parents_manager
                        .parents_at_level(header, level as BlockLevel)
                        .iter()
                        .copied()
                        .filter(|parent| self.ghostdag_stores[level].has(*parent).unwrap())
                        .collect_vec()
                        .push_if_empty(ORIGIN),
                );

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
        self.db.write(batch).unwrap();
    }

    fn estimate_proof_unique_size(&self, proof: &PruningPointProof) -> usize {
        let approx_history_size = proof[0][0].daa_score;
        let approx_unique_full_levels = f64::log2(approx_history_size as f64 / self.pruning_proof_m as f64).max(0f64) as usize;
        proof.iter().map(|l| l.len()).sum::<usize>().min((approx_unique_full_levels + 1) * self.pruning_proof_m as usize)
    }

    pub fn populate_reachability_and_headers(&self, proof: &PruningPointProof) {
        let capacity_estimate = self.estimate_proof_unique_size(proof);
        let mut dag = BlockHashMap::with_capacity(capacity_estimate);
        let mut up_heap = BinaryHeap::with_capacity(capacity_estimate);
        for header in proof.iter().flatten().cloned() {
            if let Vacant(e) = dag.entry(header.hash) {
                let state = kaspa_pow::State::new(&header);
                let (_, pow) = state.check_pow(header.nonce); // TODO: Check if pow passes
                let signed_block_level = self.max_block_level as i64 - pow.bits() as i64;
                let block_level = max(signed_block_level, 0) as BlockLevel;
                self.headers_store.insert(header.hash, header.clone(), block_level).unwrap();

                let mut parents = BlockHashSet::with_capacity(header.direct_parents().len() * 2);
                // We collect all available parent relations in order to maximize reachability information.
                // By taking into account parents from all levels we ensure that the induced DAG has valid
                // reachability information for each level-specific sub-DAG -- hence a single reachability
                // oracle can serve them all
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

        debug!("Estimated proof size: {}, actual size: {}", capacity_estimate, dag.len());

        for reverse_sortable_block in up_heap.into_sorted_iter() {
            // TODO: Convert to into_iter_sorted once it gets stable
            let hash = reverse_sortable_block.0.hash;
            let dag_entry = dag.get(&hash).unwrap();

            // Filter only existing parents
            let parents_in_dag = BinaryHeap::from_iter(
                dag_entry
                    .parents
                    .iter()
                    .cloned()
                    .filter(|parent| dag.contains_key(parent))
                    .map(|parent| SortableBlock { hash: parent, blue_work: dag.get(&parent).unwrap().header.blue_work }),
            );

            let reachability_read = self.reachability_store.upgradable_read();

            // Find the maximal parent antichain from the possibly redundant set of existing parents
            let mut reachability_parents: Vec<SortableBlock> = Vec::new();
            for parent in parents_in_dag.into_sorted_iter() {
                if reachability_read.is_dag_ancestor_of_any(parent.hash, &mut reachability_parents.iter().map(|parent| parent.hash)) {
                    continue;
                }

                reachability_parents.push(parent);
            }
            let reachability_parents_hashes =
                BlockHashes::new(reachability_parents.iter().map(|parent| parent.hash).collect_vec().push_if_empty(ORIGIN));
            let selected_parent = reachability_parents.iter().max().map(|parent| parent.hash).unwrap_or(ORIGIN);

            // Prepare batch
            let mut batch = WriteBatch::default();
            let mut reachability_relations_write = self.reachability_relations_store.write();
            let mut staging_reachability = StagingReachabilityStore::new(reachability_read);
            let mut staging_reachability_relations = StagingRelationsStore::new(&mut reachability_relations_write);

            // Stage
            staging_reachability_relations.insert(hash, reachability_parents_hashes.clone()).unwrap();
            let mergeset = unordered_mergeset_without_selected_parent(
                &staging_reachability_relations,
                &staging_reachability,
                selected_parent,
                &reachability_parents_hashes,
            );
            reachability::add_block(&mut staging_reachability, hash, selected_parent, &mut mergeset.iter().copied()).unwrap();

            // Commit
            let reachability_write = staging_reachability.commit(&mut batch).unwrap();
            staging_reachability_relations.commit(&mut batch).unwrap();

            // Write
            self.db.write(batch).unwrap();

            // Drop
            drop(reachability_write);
            drop(reachability_relations_write);
        }
    }

    pub fn validate_pruning_point_proof(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        if proof.len() != self.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(self.max_block_level as usize + 1));
        }

        let proof_pp_header = proof[0].last().expect("checked if empty");
        let proof_pp = proof_pp_header.hash;
        let proof_pp_level = calc_block_level(proof_pp_header, self.max_block_level);
        let (db_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), 2 * self.pruning_proof_m)); // TODO: Think about cache size
        let ghostdag_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(DbGhostdagStore::new(db.clone(), level, 2 * self.pruning_proof_m)))
            .collect_vec();
        let mut relations_stores =
            (0..=self.max_block_level).map(|level| DbRelationsStore::new(db.clone(), level, 2 * self.pruning_proof_m)).collect_vec();
        let reachability_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(RwLock::new(DbReachabilityStore::with_block_level(db.clone(), 2 * self.pruning_proof_m, level))))
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

                // Only the first block at each level is allowed to have no known parents
                if parents.is_empty() && i != 0 {
                    return Err(PruningImportError::PruningProofHeaderWithNoKnownParents(header.hash, level));
                }

                let parents: BlockHashes = parents.push_if_empty(ORIGIN).into();

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

        let pruning_read = self.pruning_point_store.read();
        let relations_read = self.relations_stores.read();
        let current_pp = pruning_read.get().unwrap().pruning_point;
        let current_pp_header = self.headers_store.get_header(current_pp).unwrap();

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
                let selected_tip_blue_work_diff =
                    SignedInteger::from(proof_selected_tip_gd.blue_work) - SignedInteger::from(proof_common_ancestor_gd.blue_work);
                for parent in self.parents_manager.parents_at_level(&current_pp_header, level).iter().copied() {
                    let parent_blue_work = self.ghostdag_stores[level_idx].get_blue_work(parent).unwrap();
                    let parent_blue_work_diff =
                        SignedInteger::from(parent_blue_work) - SignedInteger::from(common_ancestor_gd.blue_work);
                    if parent_blue_work_diff >= selected_tip_blue_work_diff {
                        return Err(PruningImportError::PruningProofInsufficientBlueWork);
                    }
                }

                return Ok(());
            }
        }

        if current_pp == self.genesis_hash {
            // If the proof has better tips and the current pruning point is still
            // genesis, we consider the proof state to be better.
            return Ok(());
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

    pub(crate) fn build_pruning_point_proof(&self, pp: Hash) -> PruningPointProof {
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
                let mut queue = BinaryHeap::<Reverse<SortableBlock>>::new();
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

                    headers.push(self.headers_store.get_header(current).unwrap());
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

    pub(crate) fn calculate_pruning_point_anticone_and_trusted_data(
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

            for hash in window.deref().iter().map(|block| block.0.hash) {
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
                if current == self.genesis_hash {
                    break;
                }
            }
        }

        PruningPointTrustedData {
            anticone,
            daa_window_blocks: daa_window_blocks.into_values().collect_vec(),
            ghostdag_blocks: ghostdag_blocks.into_iter().map(|(hash, ghostdag)| TrustedGhostdagData { hash, ghostdag }).collect_vec(),
        }
    }

    pub fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        let pp = self.pruning_point_store.read().pruning_point().unwrap();
        let mut cache_lock = self.cached_proof.lock();
        if let Some(cache) = cache_lock.clone() {
            if cache.pruning_point == pp {
                return cache.data;
            }
        }
        let proof = Arc::new(self.build_pruning_point_proof(pp));
        cache_lock.replace(CachedPruningPointData { pruning_point: pp, data: proof.clone() });
        proof
    }

    pub fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        let pp = self.pruning_point_store.read().pruning_point().unwrap();
        let mut cache_lock = self.cached_anticone.lock();
        if let Some(cache) = cache_lock.clone() {
            if cache.pruning_point == pp {
                return Ok(cache.data);
            }
        }

        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        let pp_bs = self.headers_store.get_blue_score(pp).unwrap();

        // The anticone is considered final only if the pruning point is at sufficient depth from virtual
        if virtual_state.ghostdag_data.blue_score >= pp_bs + self.anticone_finalization_depth {
            let anticone = Arc::new(self.calculate_pruning_point_anticone_and_trusted_data(pp, virtual_state.parents.iter().copied()));
            cache_lock.replace(CachedPruningPointData { pruning_point: pp, data: anticone.clone() });
            Ok(anticone)
        } else {
            Err(ConsensusError::PruningPointInsufficientDepth)
        }
    }
}
