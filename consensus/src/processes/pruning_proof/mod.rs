use std::{
    cmp::{max, Reverse},
    collections::{
        hash_map::Entry::{self, Vacant},
        BinaryHeap, HashSet, VecDeque,
    },
    ops::{Deref, DerefMut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use itertools::Itertools;
use kaspa_math::int::SignedInteger;
use parking_lot::{Mutex, RwLock};
use rocksdb::WriteBatch;

use kaspa_consensus_core::{
    blockhash::{self, BlockHashExtensions, BlockHashes, ORIGIN},
    errors::{
        consensus::{ConsensusError, ConsensusResult},
        pruning::{PruningImportError, PruningImportResult},
    },
    header::Header,
    pruning::{PruningPointProof, PruningPointTrustedData},
    trusted::{TrustedBlock, TrustedGhostdagData, TrustedHeader},
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
};
use kaspa_core::{debug, info, trace, warn};
use kaspa_database::{
    prelude::{CachePolicy, ConnBuilder, StoreResultEmptyTuple, StoreResultExtensions},
    utils::DbLifetime,
};
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use kaspa_utils::{binary_heap::BinaryHeapExtensions, vec::VecExtensions};
use thiserror::Error;

use crate::{
    consensus::{
        services::{DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbWindowManager},
        storage::ConsensusStorage,
    },
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            depth::DbDepthStore,
            ghostdag::{CompactGhostdagData, DbGhostdagStore, GhostdagData, GhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader, HeaderWithBlockLevel},
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

#[derive(Error, Debug)]
enum PruningProofManagerInternalError {
    #[error("block at depth error: {0}")]
    BlockAtDepth(String),

    #[error("find common ancestor error: {0}")]
    FindCommonAncestor(String),

    #[error("cannot find a common ancestor: {0}")]
    NoCommonAncestor(String),

    #[error("missing headers to build proof: {0}")]
    NotEnoughHeadersToBuildProof(String),
}
type PruningProofManagerInternalResult<T> = std::result::Result<T, PruningProofManagerInternalError>;

struct CachedPruningPointData<T: ?Sized> {
    pruning_point: Hash,
    data: Arc<T>,
}

impl<T> Clone for CachedPruningPointData<T> {
    fn clone(&self) -> Self {
        Self { pruning_point: self.pruning_point, data: self.data.clone() }
    }
}

struct TempProofContext {
    headers_store: Arc<DbHeadersStore>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    relations_stores: Vec<DbRelationsStore>,
    reachability_stores: Vec<Arc<parking_lot::lock_api::RwLock<parking_lot::RawRwLock, DbReachabilityStore>>>,
    ghostdag_managers:
        Vec<GhostdagManager<DbGhostdagStore, DbRelationsStore, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>>,
    db_lifetime: DbLifetime,
}

pub struct PruningProofManager {
    db: Arc<DB>,

    headers_store: Arc<DbHeadersStore>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    reachability_relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    ghostdag_primary_store: Arc<DbGhostdagStore>,
    relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    pruning_point_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    virtual_stores: Arc<RwLock<VirtualStores>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    depth_store: Arc<DbDepthStore>,
    selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,

    ghostdag_primary_manager: DbGhostdagManager,
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

    is_consensus_exiting: Arc<AtomicBool>,
}

impl PruningProofManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        parents_manager: DbParentsManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        ghostdag_manager: DbGhostdagManager,
        traversal_manager: DbDagTraversalManager,
        window_manager: DbWindowManager,
        max_block_level: BlockLevel,
        genesis_hash: Hash,
        pruning_proof_m: u64,
        anticone_finalization_depth: u64,
        ghostdag_k: KType,
        is_consensus_exiting: Arc<AtomicBool>,
    ) -> Self {
        Self {
            db,
            headers_store: storage.headers_store.clone(),
            reachability_store: storage.reachability_store.clone(),
            reachability_relations_store: storage.reachability_relations_store.clone(),
            reachability_service,
            ghostdag_primary_store: storage.ghostdag_primary_store.clone(),
            relations_stores: storage.relations_stores.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            headers_selected_tip_store: storage.headers_selected_tip_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            depth_store: storage.depth_store.clone(),

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
            ghostdag_primary_manager: ghostdag_manager,

            is_consensus_exiting,
        }
    }

    pub fn import_pruning_points(&self, pruning_points: &[Arc<Header>]) {
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
        info!("Setting {new_pruning_point} as the staging pruning point");

        let mut pruning_point_write = self.pruning_point_store.write();
        let mut batch = WriteBatch::default();
        pruning_point_write.set_batch(&mut batch, new_pruning_point, new_pruning_point, (pruning_points.len() - 1) as u64).unwrap();
        pruning_point_write.set_history_root(&mut batch, new_pruning_point).unwrap();
        self.db.write(batch).unwrap();
        drop(pruning_point_write);
    }

    pub fn apply_proof(&self, mut proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
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

        {
            let reachability_read = self.reachability_store.read();
            for tb in trusted_set.iter() {
                // Header-only trusted blocks are expected to be in pruning point past
                if tb.block.is_header_only() && !reachability_read.is_dag_ancestor_of(tb.block.hash(), pruning_point) {
                    return Err(PruningImportError::PruningPointPastMissingReachability(tb.block.hash()));
                }
            }
        }

        for (level, headers) in proof.iter().enumerate() {
            trace!("Applying level {} from the pruning point proof", level);
            let mut level_ancestors: HashSet<Hash> = HashSet::new();
            level_ancestors.insert(ORIGIN);

            for header in headers.iter() {
                let parents = Arc::new(
                    self.parents_manager
                        .parents_at_level(header, level as BlockLevel)
                        .iter()
                        .copied()
                        .filter(|parent| level_ancestors.contains(parent))
                        .collect_vec()
                        .push_if_empty(ORIGIN),
                );

                self.relations_stores.write()[level].insert(header.hash, parents.clone()).unwrap();

                if level == 0 {
                    // self.ghostdag_primary_store.insert(ORIGIN, self.ghostdag_primary_manager.origin_ghostdag_data()).unwrap();

                    let gd = if let Some(gd) = trusted_gd_map.get(&header.hash) {
                        gd.clone()
                    } else {
                        let calculated_gd = self.ghostdag_primary_manager.ghostdag(&parents);
                        // Override the ghostdag data with the real blue score and blue work
                        GhostdagData {
                            blue_score: header.blue_score,
                            blue_work: header.blue_work,
                            selected_parent: calculated_gd.selected_parent,
                            mergeset_blues: calculated_gd.mergeset_blues.clone(),
                            mergeset_reds: calculated_gd.mergeset_reds.clone(),
                            blues_anticone_sizes: calculated_gd.blues_anticone_sizes.clone(),
                        }
                    };
                    self.ghostdag_primary_store.insert(header.hash, Arc::new(gd)).unwrap();
                }

                level_ancestors.insert(header.hash);
            }
        }

        let virtual_parents = vec![pruning_point];
        let virtual_state = Arc::new(VirtualState {
            parents: virtual_parents.clone(),
            ghostdag_data: self.ghostdag_primary_manager.ghostdag(&virtual_parents),
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
        self.depth_store.insert_batch(&mut batch, pruning_point, ORIGIN, ORIGIN).unwrap();
        self.db.write(batch).unwrap();

        Ok(())
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

    fn init_validate_pruning_point_proof_stores_and_processes(
        &self,
        proof: &PruningPointProof,
    ) -> PruningImportResult<TempProofContext> {
        if proof[0].is_empty() {
            return Err(PruningImportError::PruningProofNotEnoughHeaders);
        }

        let headers_estimate = self.estimate_proof_unique_size(proof);

        let (db_lifetime, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);
        let headers_store =
            Arc::new(DbHeadersStore::new(db.clone(), CachePolicy::Count(headers_estimate), CachePolicy::Count(headers_estimate)));
        let ghostdag_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(DbGhostdagStore::new(db.clone(), level, cache_policy, cache_policy)))
            .collect_vec();
        let mut relations_stores =
            (0..=self.max_block_level).map(|level| DbRelationsStore::new(db.clone(), level, cache_policy, cache_policy)).collect_vec();
        let reachability_stores = (0..=self.max_block_level)
            .map(|level| Arc::new(RwLock::new(DbReachabilityStore::with_block_level(db.clone(), cache_policy, cache_policy, level))))
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
                    level != 0,
                )
            })
            .collect_vec();

        {
            let mut batch = WriteBatch::default();
            for level in 0..=self.max_block_level {
                let level = level as usize;
                reachability::init(reachability_stores[level].write().deref_mut()).unwrap();
                relations_stores[level].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap();
                ghostdag_stores[level].insert(ORIGIN, ghostdag_managers[level].origin_ghostdag_data()).unwrap();
            }

            db.write(batch).unwrap();
        }

        Ok(TempProofContext { db_lifetime, headers_store, ghostdag_stores, relations_stores, reachability_stores, ghostdag_managers })
    }

    fn populate_stores_for_validate_pruning_point_proof(
        &self,
        proof: &PruningPointProof,
        stores_and_processes: &mut TempProofContext,
    ) -> PruningImportResult<Vec<Hash>> {
        let headers_store = &stores_and_processes.headers_store;
        let ghostdag_stores = &stores_and_processes.ghostdag_stores;
        let mut relations_stores = stores_and_processes.relations_stores.clone();
        let reachability_stores = &stores_and_processes.reachability_stores;
        let ghostdag_managers = &stores_and_processes.ghostdag_managers;

        let proof_pp_header = proof[0].last().expect("checked if empty");
        let proof_pp = proof_pp_header.hash;

        let mut selected_tip_by_level = vec![None; self.max_block_level as usize + 1];
        for level in (0..=self.max_block_level).rev() {
            // Before processing this level, check if the process is exiting so we can end early
            if self.is_consensus_exiting.load(Ordering::Relaxed) {
                return Err(PruningImportError::PruningValidationInterrupted);
            }

            info!("Validating level {level} from the pruning point proof ({} headers)", proof[level as usize].len());
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
                let block_at_depth_m_at_next_level = self
                    .block_at_depth(
                        &*ghostdag_stores[level_idx + 1],
                        selected_tip_by_level[level_idx + 1].unwrap(),
                        self.pruning_proof_m,
                    )
                    .unwrap();
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

        Ok(selected_tip_by_level.into_iter().map(|selected_tip| selected_tip.unwrap()).collect())
    }

    fn validate_proof_selected_tip(
        &self,
        proof_selected_tip: Hash,
        level: BlockLevel,
        proof_pp_level: BlockLevel,
        proof_pp: Hash,
        proof_pp_header: &Header,
    ) -> PruningImportResult<()> {
        // A proof selected tip of some level has to be the proof suggested prunint point itself if its level
        // is lower or equal to the pruning point level, or a parent of the pruning point on the relevant level
        // otherwise.
        if level <= proof_pp_level {
            if proof_selected_tip != proof_pp {
                return Err(PruningImportError::PruningProofSelectedTipIsNotThePruningPoint(proof_selected_tip, level));
            }
        } else if !self.parents_manager.parents_at_level(proof_pp_header, level).contains(&proof_selected_tip) {
            return Err(PruningImportError::PruningProofSelectedTipNotParentOfPruningPoint(proof_selected_tip, level));
        }

        Ok(())
    }

    // find_proof_and_consensus_common_chain_ancestor_ghostdag_data returns an option of a tuple
    // that contains the ghostdag data of the proof and current consensus common ancestor. If no
    // such ancestor exists, it returns None.
    fn find_proof_and_consensus_common_ancestor_ghostdag_data(
        &self,
        ghostdag_stores: &[Arc<DbGhostdagStore>],
        proof_selected_tip: Hash,
        level: BlockLevel,
        proof_selected_tip_gd: CompactGhostdagData,
    ) -> Option<(CompactGhostdagData, CompactGhostdagData)> {
        let mut proof_current = proof_selected_tip;
        let mut proof_current_gd = proof_selected_tip_gd;
        loop {
            match ghostdag_stores[level as usize].get_compact_data(proof_current).unwrap_option() {
                Some(current_gd) => {
                    break Some((proof_current_gd, current_gd));
                }
                None => {
                    proof_current = proof_current_gd.selected_parent;
                    if proof_current.is_origin() {
                        break None;
                    }
                    proof_current_gd = ghostdag_stores[level as usize].get_compact_data(proof_current).unwrap();
                }
            };
        }
    }

    pub fn validate_pruning_point_proof(&self, proof: &PruningPointProof) -> PruningImportResult<()> {
        if proof.len() != self.max_block_level as usize + 1 {
            return Err(PruningImportError::ProofNotEnoughLevels(self.max_block_level as usize + 1));
        }

        let proof_pp_header = proof[0].last().expect("checked if empty");
        let proof_pp = proof_pp_header.hash;
        let proof_pp_level = calc_block_level(proof_pp_header, self.max_block_level);
        let mut stores_and_processes = self.init_validate_pruning_point_proof_stores_and_processes(&proof)?;
        let selected_tip_by_level = self.populate_stores_for_validate_pruning_point_proof(proof, &mut stores_and_processes)?;
        let ghostdag_stores = stores_and_processes.ghostdag_stores;

        let pruning_read = self.pruning_point_store.read();
        let relations_read = self.relations_stores.read();
        let current_pp = pruning_read.get().unwrap().pruning_point;
        let current_pp_header = self.headers_store.get_header(current_pp).unwrap();

        for (level_idx, selected_tip) in selected_tip_by_level.iter().copied().enumerate() {
            let level = level_idx as BlockLevel;
            self.validate_proof_selected_tip(selected_tip, level, proof_pp_level, proof_pp, proof_pp_header)?;

            let proof_selected_tip_gd = ghostdag_stores[level_idx].get_compact_data(selected_tip).unwrap();
            if proof_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            if let Some((proof_common_ancestor_gd, common_ancestor_gd)) = self.find_proof_and_consensus_common_ancestor_ghostdag_data(
                &ghostdag_stores,
                selected_tip,
                level,
                proof_selected_tip_gd,
            ) {
                let selected_tip_blue_work_diff =
                    SignedInteger::from(proof_selected_tip_gd.blue_work) - SignedInteger::from(proof_common_ancestor_gd.blue_work);
                for parent in self.parents_manager.parents_at_level(&current_pp_header, level).iter().copied() {
                    let parent_blue_work = ghostdag_stores[level_idx].get_blue_work(parent).unwrap();
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

        // If we got here it means there's no level with shared blocks
        // between the proof and the current consensus. In this case we
        // consider the proof to be better if it has at least one level
        // with 2*self.pruning_proof_m blue blocks where consensus doesn't.
        for level in (0..=self.max_block_level).rev() {
            let level_idx = level as usize;

            let proof_selected_tip = selected_tip_by_level[level_idx];
            let proof_selected_tip_gd = ghostdag_stores[level_idx].get_compact_data(proof_selected_tip).unwrap();
            if proof_selected_tip_gd.blue_score < 2 * self.pruning_proof_m {
                continue;
            }

            match relations_read[level_idx].get_parents(current_pp).unwrap_option() {
                Some(parents) => {
                    if parents
                        .iter()
                        .copied()
                        .any(|parent| ghostdag_stores[level_idx].get_blue_score(parent).unwrap() < 2 * self.pruning_proof_m)
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
        drop(stores_and_processes.db_lifetime);

        Err(PruningImportError::PruningProofNotEnoughHeaders)
    }

    // The "current dag level" is the level right before the level whose parents are
    // not the same as our header's direct parents
    //
    // Find the current DAG level by going through all the parents at each level,
    // starting from the bottom level and see which is the first level that has
    // parents that are NOT our current pp_header's direct parents.
    fn find_current_dag_level(&self, pp_header: &Header) -> BlockLevel {
        let direct_parents = BlockHashSet::from_iter(pp_header.direct_parents().iter().copied());
        pp_header
            .parents_by_level
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(level, parents)| {
                if BlockHashSet::from_iter(parents.iter().copied()) == direct_parents {
                    None
                } else {
                    Some((level - 1) as BlockLevel)
                }
            })
            .unwrap_or(self.max_block_level)
    }

    fn estimated_blue_depth_at_level_0(&self, level: BlockLevel, level_depth: u64, current_dag_level: BlockLevel) -> u64 {
        level_depth << current_dag_level.saturating_sub(level)
    }

    /// selected parent at level = the parent of the header at the level
    /// with the highest blue_work (using score as work in this case)
    fn find_selected_parent_header_at_level(
        &self,
        header: &Header,
        level: BlockLevel,
        relations_service: MTRelationsService<DbRelationsStore>,
    ) -> PruningProofManagerInternalResult<Arc<Header>> {
        // Logic of apply_proof only inserts parent entries for a header from the proof
        // into the relations store for a level if there was GD data in the old stores for that
        // header. To mimic that logic here, we need to filter out parents that are NOT in the relations_service
        let parents = self
            .parents_manager
            .parents_at_level(header, level)
            .iter()
            .copied()
            .filter(|parent| relations_service.has(*parent).unwrap())
            .collect_vec()
            .push_if_empty(ORIGIN);

        let mut sp = SortableBlock { hash: parents[0], blue_work: self.headers_store.get_blue_score(parents[0]).unwrap_or(0).into() };
        for parent in parents.iter().copied().skip(1) {
            let sblock = SortableBlock {
                hash: parent,
                blue_work: self
                    .headers_store
                    .get_blue_score(parent)
                    .unwrap_option()
                    .ok_or(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof(format!(
                        "find_selected_parent_header_at_level (level {level}) couldn't find the header for block {parent}"
                    )))?
                    .into(),
            };
            if sblock > sp {
                sp = sblock;
            }
        }
        // TODO: For higher levels the chance of having more than two parents is very small, so it might make sense to fetch the whole header for the SortableBlock instead of blue_score (which will probably come from a compact header).
        self.headers_store.get_header(sp.hash).unwrap_option().ok_or(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof(
            format!("find_selected_parent_header_at_level (level {level}) couldn't find the header for block {}", sp.hash,),
        ))
        // Ok(self.headers_store.get_header(sp.hash).unwrap_option().expect("already checked if compact header exists above"))
    }

    fn find_sufficient_root(
        &self,
        pp_header: &HeaderWithBlockLevel,
        level: BlockLevel,
        required_block: Option<Hash>,
        temp_db: Arc<DB>,
    ) -> PruningProofManagerInternalResult<(Arc<DbGhostdagStore>, Hash, Hash)> {
        let relations_service = MTRelationsService::new(self.relations_stores.clone(), level);
        let selected_tip_header = if pp_header.block_level >= level {
            pp_header.header.clone()
        } else {
            self.find_selected_parent_header_at_level(&pp_header.header, level, relations_service.clone())?
        };

        let selected_tip = selected_tip_header.hash;
        let pp = pp_header.header.hash;

        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize); // TODO: We can probably reduce cache size
        let required_level_depth = 2 * self.pruning_proof_m;
        let mut required_level_0_depth = required_level_depth;
        // let mut required_level_0_depth = if level == 0 {
        //     required_level_depth
        // } else {
        //     self.estimated_blue_depth_at_level_0(
        //         level,
        //         required_level_depth * 5 / 4, // We take a safety margin
        //         current_dag_level,
        //     )
        // };

        let mut tries = 0;
        loop {
            let required_block = if let Some(required_block) = required_block {
                // TODO: We can change it to skip related checks if `None`
                required_block
            } else {
                selected_tip
            };

            let mut finished_headers = false;
            let mut finished_headers_for_required_block_chain = false;
            let mut current_header = selected_tip_header.clone();
            let mut required_block_chain = BlockHashSet::new();
            let mut selected_chain = BlockHashSet::new();
            let mut intersected_with_required_block_chain = false;
            let mut current_required_chain_block = self.headers_store.get_header(required_block).unwrap();
            let root_header = loop {
                if !intersected_with_required_block_chain {
                    required_block_chain.insert(current_required_chain_block.hash);
                    selected_chain.insert(current_header.hash);
                    if required_block_chain.contains(&current_header.hash)
                        || selected_chain.contains(&current_required_chain_block.hash)
                    {
                        intersected_with_required_block_chain = true;
                    }
                }

                if current_header.direct_parents().is_empty() // Stop at genesis
                    // Need to ensure this does the same 2M+1 depth that block_at_depth does
                    || (pp_header.header.blue_score > current_header.blue_score + required_level_0_depth
                        && intersected_with_required_block_chain)
                {
                    break current_header;
                }
                current_header = match self.find_selected_parent_header_at_level(&current_header, level, relations_service.clone()) {
                    Ok(header) => header,
                    Err(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof(_)) => {
                        if !intersected_with_required_block_chain {
                            warn!("it's unknown if the selected root for level {level} ( {} ) is in the chain of the required block {required_block}", current_header.hash)
                        }
                        finished_headers = true; // We want to give this root a shot if all its past is pruned
                        break current_header;
                    }
                    Err(e) => return Err(e),
                };

                if !finished_headers_for_required_block_chain && !intersected_with_required_block_chain {
                    current_required_chain_block = match self.find_selected_parent_header_at_level(
                        &current_required_chain_block,
                        level,
                        relations_service.clone(),
                    ) {
                        Ok(header) => header,
                        Err(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof(_)) => {
                            finished_headers_for_required_block_chain = true;
                            current_required_chain_block
                        }
                        Err(e) => return Err(e),
                    };
                }
            };
            let root = root_header.hash;

            if level == 0 {
                return Ok((self.ghostdag_primary_store.clone(), selected_tip, root));
            }

            let ghostdag_store = Arc::new(DbGhostdagStore::new_temp(temp_db.clone(), level, cache_policy, cache_policy, tries));
            let gd_manager = GhostdagManager::new(
                root,
                self.ghostdag_k,
                ghostdag_store.clone(),
                relations_service.clone(),
                self.headers_store.clone(),
                self.reachability_service.clone(),
                true,
            );
            ghostdag_store.insert(root, Arc::new(gd_manager.genesis_ghostdag_data())).unwrap();
            ghostdag_store.insert(ORIGIN, gd_manager.origin_ghostdag_data()).unwrap();
            let mut topological_heap: BinaryHeap<_> = Default::default();
            let mut visited = BlockHashSet::new();
            for child in relations_service.get_children(root).unwrap().read().iter().copied() {
                topological_heap.push(Reverse(SortableBlock {
                    hash: child,
                    // It's important to use here blue work and not score so we can iterate the heap in a way that respects the topology
                    blue_work: self.headers_store.get_header(child).unwrap().blue_work, // TODO: Maybe add to compact data?
                }));
            }

            let mut has_required_block = root == required_block;
            loop {
                let Some(current) = topological_heap.pop() else {
                    break;
                };
                let current_hash = current.0.hash;
                if !visited.insert(current_hash) {
                    continue;
                }

                if !self.reachability_service.is_dag_ancestor_of(current_hash, pp) {
                    // We don't care about blocks in the antipast of the pruning point
                    continue;
                }

                if !has_required_block && current_hash == required_block {
                    has_required_block = true;
                }

                let relevant_parents: Box<[Hash]> = relations_service
                    .get_parents(current_hash)
                    .unwrap()
                    .iter()
                    .copied()
                    .filter(|parent| self.reachability_service.is_dag_ancestor_of(root, *parent))
                    .collect();
                let current_gd = gd_manager.ghostdag(&relevant_parents);
                ghostdag_store.insert(current_hash, Arc::new(current_gd)).unwrap();
                for child in relations_service.get_children(current_hash).unwrap().read().iter().copied() {
                    topological_heap.push(Reverse(SortableBlock {
                        hash: child,
                        // It's important to use here blue work and not score so we can iterate the heap in a way that respects the topology
                        blue_work: self.headers_store.get_header(child).unwrap().blue_work, // TODO: Maybe add to compact data?
                    }));
                }
            }

            // Need to ensure this does the same 2M+1 depth that block_at_depth does
            if has_required_block
                && (root == self.genesis_hash || ghostdag_store.get_blue_score(selected_tip).unwrap() > required_level_depth)
            {
                break Ok((ghostdag_store, selected_tip, root));
            }

            tries += 1;
            if finished_headers {
                warn!("Failed to find sufficient root for level {level} after {tries} tries. Headers below the current depth of {required_level_0_depth} are already pruned. Trying anyway.");
                break Ok((ghostdag_store, selected_tip, root));
            }
            required_level_0_depth <<= 1;
            warn!("Failed to find sufficient root for level {level} after {tries} tries. Retrying again to find with depth {required_level_0_depth}");
        }
    }

    fn calc_gd_for_all_levels(
        &self,
        pp_header: &HeaderWithBlockLevel,
        temp_db: Arc<DB>,
    ) -> (Vec<Arc<DbGhostdagStore>>, Vec<Hash>, Vec<Hash>) {
        let mut ghostdag_stores: Vec<Option<Arc<DbGhostdagStore>>> = vec![None; self.max_block_level as usize + 1];
        let mut selected_tip_by_level = vec![None; self.max_block_level as usize + 1];
        let mut root_by_level = vec![None; self.max_block_level as usize + 1];
        for level in (0..=self.max_block_level).rev() {
            let level_usize = level as usize;
            let required_block = if level != self.max_block_level {
                let next_level_store = ghostdag_stores[level_usize + 1].as_ref().unwrap().clone();
                let block_at_depth_m_at_next_level = self
                    .block_at_depth(&*next_level_store, selected_tip_by_level[level_usize + 1].unwrap(), self.pruning_proof_m)
                    .map_err(|err| format!("level + 1: {}, err: {}", level + 1, err))
                    .unwrap();
                Some(block_at_depth_m_at_next_level)
            } else {
                None
            };
            let (store, selected_tip, root) = self
                .find_sufficient_root(&pp_header, level, required_block, temp_db.clone())
                .expect(&format!("find_sufficient_root failed for level {level}"));
            ghostdag_stores[level_usize] = Some(store);
            selected_tip_by_level[level_usize] = Some(selected_tip);
            root_by_level[level_usize] = Some(root);
        }

        (
            ghostdag_stores.into_iter().map(Option::unwrap).collect_vec(),
            selected_tip_by_level.into_iter().map(Option::unwrap).collect_vec(),
            root_by_level.into_iter().map(Option::unwrap).collect_vec(),
        )
    }

    pub(crate) fn build_pruning_point_proof(&self, pp: Hash) -> PruningPointProof {
        if pp == self.genesis_hash {
            return vec![];
        }

        let (_db_lifetime, temp_db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let pp_header = self.headers_store.get_header_with_block_level(pp).unwrap();
        let (ghostdag_stores, selected_tip_by_level, roots_by_level) = self.calc_gd_for_all_levels(&pp_header, temp_db);

        (0..=self.max_block_level)
            .map(|level| {
                let level = level as usize;
                let selected_tip = selected_tip_by_level[level];
                let block_at_depth_2m = self
                    .block_at_depth(&*ghostdag_stores[level], selected_tip, 2 * self.pruning_proof_m)
                    .map_err(|err| format!("level: {}, err: {}", level, err))
                    .unwrap();

                let root = roots_by_level[level];
                let old_root = if level != self.max_block_level as usize {
                    let block_at_depth_m_at_next_level = self
                        .block_at_depth(&*ghostdag_stores[level + 1], selected_tip_by_level[level + 1], self.pruning_proof_m)
                        .map_err(|err| format!("level + 1: {}, err: {}", level + 1, err))
                        .unwrap();
                    if self.reachability_service.is_dag_ancestor_of(block_at_depth_m_at_next_level, block_at_depth_2m) {
                        block_at_depth_m_at_next_level
                    } else if self.reachability_service.is_dag_ancestor_of(block_at_depth_2m, block_at_depth_m_at_next_level) {
                        block_at_depth_2m
                    } else {
                        self.find_common_ancestor_in_chain_of_a(
                            &*ghostdag_stores[level],
                            block_at_depth_m_at_next_level,
                            block_at_depth_2m,
                        )
                        .map_err(|err| format!("level: {}, err: {}", level, err))
                        .unwrap()
                    }
                } else {
                    block_at_depth_2m
                };
                assert!(self.reachability_service.is_dag_ancestor_of(root, old_root));

                let mut headers = Vec::with_capacity(2 * self.pruning_proof_m as usize);
                let mut queue = BinaryHeap::<Reverse<SortableBlock>>::new();
                let mut visited = BlockHashSet::new();
                queue.push(Reverse(SortableBlock::new(root, self.headers_store.get_header(root).unwrap().blue_work)));
                while let Some(current) = queue.pop() {
                    let current = current.0.hash;
                    if !visited.insert(current) {
                        continue;
                    }

                    if !self.reachability_service.is_dag_ancestor_of(current, selected_tip) {
                        continue;
                    }

                    headers.push(self.headers_store.get_header(current).unwrap());
                    for child in self.relations_stores.read()[level].get_children(current).unwrap().read().iter().copied() {
                        queue.push(Reverse(SortableBlock::new(child, self.headers_store.get_header(child).unwrap().blue_work)));
                    }
                }

                // Temp assertion for verifying a bug fix: assert that the full 2M chain is actually contained in the composed level proof
                let set = BlockHashSet::from_iter(headers.iter().map(|h| h.hash));
                let chain_2m = self
                    .chain_up_to_depth(&*ghostdag_stores[level], selected_tip, 2 * self.pruning_proof_m)
                    .map_err(|err| {
                        dbg!(level, selected_tip, block_at_depth_2m, root);
                        format!("Assert 2M chain -- level: {}, err: {}", level, err)
                    })
                    .unwrap();
                let chain_2m_len = chain_2m.len();
                for (i, chain_hash) in chain_2m.into_iter().enumerate() {
                    if !set.contains(&chain_hash) {
                        let next_level_tip = selected_tip_by_level[level + 1];
                        let next_level_chain_m =
                            self.chain_up_to_depth(&*ghostdag_stores[level + 1], next_level_tip, self.pruning_proof_m).unwrap();
                        let next_level_block_m = next_level_chain_m.last().copied().unwrap();
                        dbg!(next_level_chain_m.len());
                        dbg!(ghostdag_stores[level + 1].get_compact_data(next_level_tip).unwrap().blue_score);
                        dbg!(ghostdag_stores[level + 1].get_compact_data(next_level_block_m).unwrap().blue_score);
                        dbg!(ghostdag_stores[level].get_compact_data(selected_tip).unwrap().blue_score);
                        dbg!(ghostdag_stores[level].get_compact_data(block_at_depth_2m).unwrap().blue_score);
                        dbg!(level, selected_tip, block_at_depth_2m, root);
                        panic!("Assert 2M chain -- missing block {} at index {} out of {} chain blocks", chain_hash, i, chain_2m_len);
                    }
                }

                headers
            })
            .collect_vec()
    }

    /// Copy of `block_at_depth` which returns the full chain up to depth. Temporarily used for assertion purposes.
    fn chain_up_to_depth(
        &self,
        ghostdag_store: &impl GhostdagStoreReader,
        high: Hash,
        depth: u64,
    ) -> Result<Vec<Hash>, PruningProofManagerInternalError> {
        let high_gd = ghostdag_store
            .get_compact_data(high)
            .map_err(|err| PruningProofManagerInternalError::BlockAtDepth(format!("high: {high}, depth: {depth}, {err}")))?;
        let mut current_gd = high_gd;
        let mut current = high;
        let mut res = vec![current];
        while current_gd.blue_score + depth >= high_gd.blue_score {
            if current_gd.selected_parent.is_origin() {
                break;
            }
            let prev = current;
            current = current_gd.selected_parent;
            res.push(current);
            current_gd = ghostdag_store.get_compact_data(current).map_err(|err| {
                PruningProofManagerInternalError::BlockAtDepth(format!(
                    "high: {}, depth: {}, current: {}, high blue score: {}, current blue score: {}, {}",
                    high, depth, prev, high_gd.blue_score, current_gd.blue_score, err
                ))
            })?;
        }
        Ok(res)
    }

    fn block_at_depth(
        &self,
        ghostdag_store: &impl GhostdagStoreReader,
        high: Hash,
        depth: u64,
    ) -> Result<Hash, PruningProofManagerInternalError> {
        let high_gd = ghostdag_store
            .get_compact_data(high)
            .map_err(|err| PruningProofManagerInternalError::BlockAtDepth(format!("high: {high}, depth: {depth}, {err}")))?;
        let mut current_gd = high_gd;
        let mut current = high;
        while current_gd.blue_score + depth >= high_gd.blue_score {
            if current_gd.selected_parent.is_origin() {
                break;
            }
            let prev = current;
            current = current_gd.selected_parent;
            current_gd = ghostdag_store.get_compact_data(current).map_err(|err| {
                PruningProofManagerInternalError::BlockAtDepth(format!(
                    "high: {}, depth: {}, current: {}, high blue score: {}, current blue score: {}, {}",
                    high, depth, prev, high_gd.blue_score, current_gd.blue_score, err
                ))
            })?;
        }
        Ok(current)
    }

    fn find_common_ancestor_in_chain_of_a(
        &self,
        ghostdag_store: &impl GhostdagStoreReader,
        a: Hash,
        b: Hash,
    ) -> Result<Hash, PruningProofManagerInternalError> {
        let a_gd = ghostdag_store
            .get_compact_data(a)
            .map_err(|err| PruningProofManagerInternalError::FindCommonAncestor(format!("a: {a}, b: {b}, {err}")))?;
        let mut current_gd = a_gd;
        let mut current;
        let mut loop_counter = 0;
        loop {
            current = current_gd.selected_parent;
            loop_counter += 1;
            if current.is_origin() {
                break Err(PruningProofManagerInternalError::NoCommonAncestor(format!("a: {a}, b: {b} ({loop_counter} loop steps)")));
            }
            if self.reachability_service.is_dag_ancestor_of(current, b) {
                break Ok(current);
            }
            current_gd = ghostdag_store
                .get_compact_data(current)
                .map_err(|err| PruningProofManagerInternalError::FindCommonAncestor(format!("a: {a}, b: {b}, {err}")))?;
        }
    }

    /// Returns the k + 1 chain blocks below this hash (inclusive). If data is missing
    /// the search is halted and a partial chain is returned.
    ///
    /// The returned hashes are guaranteed to have GHOSTDAG data
    pub(crate) fn get_ghostdag_chain_k_depth(&self, hash: Hash) -> Vec<Hash> {
        let mut hashes = Vec::with_capacity(self.ghostdag_k as usize + 1);
        let mut current = hash;
        for _ in 0..=self.ghostdag_k {
            hashes.push(current);
            let Some(parent) = self.ghostdag_primary_store.get_selected_parent(current).unwrap_option() else {
                break;
            };
            if parent == self.genesis_hash || parent == blockhash::ORIGIN {
                break;
            }
            current = parent;
        }
        hashes
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
        let mut anticone = self.ghostdag_primary_manager.sort_blocks(anticone);
        anticone.insert(0, pruning_point);

        let mut daa_window_blocks = BlockHashMap::new();
        let mut ghostdag_blocks = BlockHashMap::new();

        // PRUNE SAFETY: called either via consensus under the prune guard or by the pruning processor (hence no pruning in parallel)

        for anticone_block in anticone.iter().copied() {
            let window = self
                .window_manager
                .block_window(&self.ghostdag_primary_store.get_data(anticone_block).unwrap(), WindowType::FullDifficultyWindow)
                .unwrap();

            for hash in window.deref().iter().map(|block| block.0.hash) {
                if let Entry::Vacant(e) = daa_window_blocks.entry(hash) {
                    e.insert(TrustedHeader {
                        header: self.headers_store.get_header(hash).unwrap(),
                        ghostdag: (&*self.ghostdag_primary_store.get_data(hash).unwrap()).into(),
                    });
                }
            }

            let ghostdag_chain = self.get_ghostdag_chain_k_depth(anticone_block);
            for hash in ghostdag_chain {
                if let Entry::Vacant(e) = ghostdag_blocks.entry(hash) {
                    let ghostdag = self.ghostdag_primary_store.get_data(hash).unwrap();
                    e.insert((&*ghostdag).into());

                    // We fill `ghostdag_blocks` only for kaspad-go legacy reasons, but the real set we
                    // send is `daa_window_blocks` which represents the full trusted sub-DAG in the antifuture
                    // of the pruning point which kaspad-rust nodes expect to get when synced with headers proof
                    if let Entry::Vacant(e) = daa_window_blocks.entry(hash) {
                        e.insert(TrustedHeader {
                            header: self.headers_store.get_header(hash).unwrap(),
                            ghostdag: (&*ghostdag).into(),
                        });
                    }
                }
            }
        }

        // We traverse the DAG in the past of the pruning point and its anticone in order to make sure
        // that the sub-DAG we share (which contains the union of DAA windows), is contiguous and includes
        // all blocks between the pruning point and the DAA window blocks. This is crucial for the syncee
        // to be able to build full reachability data of the sub-DAG and to actually validate that only the
        // claimed anticone is indeed the pp anticone and all the rest of the blocks are in the pp past.

        // We use the min blue-work in order to identify where the traversal can halt
        let min_blue_work = daa_window_blocks.values().map(|th| th.header.blue_work).min().expect("non empty");
        let mut queue = VecDeque::from_iter(anticone.iter().copied());
        let mut visited = BlockHashSet::from_iter(queue.iter().copied().chain(std::iter::once(blockhash::ORIGIN))); // Mark origin as visited to avoid processing it
        while let Some(current) = queue.pop_front() {
            if let Entry::Vacant(e) = daa_window_blocks.entry(current) {
                let header = self.headers_store.get_header(current).unwrap();
                if header.blue_work < min_blue_work {
                    continue;
                }
                let ghostdag = (&*self.ghostdag_primary_store.get_data(current).unwrap()).into();
                e.insert(TrustedHeader { header, ghostdag });
            }
            let parents = self.relations_stores.read()[0].get_parents(current).unwrap();
            for parent in parents.iter().copied() {
                if visited.insert(parent) {
                    queue.push_back(parent);
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
