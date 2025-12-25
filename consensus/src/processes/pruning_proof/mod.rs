mod apply;
mod build;
mod validate;

use std::{
    collections::{
        hash_map::Entry::{self},
        VecDeque,
    },
    sync::{atomic::AtomicBool, Arc},
};

use itertools::Itertools;
use parking_lot::{Mutex, RwLock};
use rocksdb::WriteBatch;

use kaspa_consensus_core::{
    blockhash::{self, BlockHashExtensions},
    errors::{
        consensus::{ConsensusError, ConsensusResult},
        pruning::{PruningImportError, PruningImportResult},
    },
    header::Header,
    pruning::{PruningPointProof, PruningPointTrustedData},
    trusted::{TrustedGhostdagData, TrustedHeader},
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
};
use kaspa_core::info;
use kaspa_database::{prelude::StoreResultExtensions, utils::DbLifetime};
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use thiserror::Error;

use crate::{
    consensus::{
        services::{DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbWindowManager},
        storage::ConsensusStorage,
    },
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStore, HeaderStoreReader},
            headers_selected_tip::DbHeadersSelectedTipStore,
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore},
            pruning::{DbPruningStore, PruningStoreReader},
            pruning_meta::PruningMetaStores,
            pruning_samples::{DbPruningSamplesStore, PruningSamplesStore},
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            selected_chain::DbSelectedChainStore,
            tips::DbTipsStore,
            virtual_state::{VirtualStateStoreReader, VirtualStores},
            DB,
        },
    },
    processes::window::WindowType,
};

use super::{ghostdag::protocol::GhostdagManager, window::WindowManager};

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
    reachability_stores: Vec<Arc<RwLock<DbReachabilityStore>>>,
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
    ghostdag_store: Arc<DbGhostdagStore>,
    relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    level_relations_services: Vec<MTRelationsService<DbRelationsStore>>,
    pruning_point_store: Arc<RwLock<DbPruningStore>>,
    past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    virtual_stores: Arc<RwLock<VirtualStores>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    depth_store: Arc<DbDepthStore>,
    selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
    pruning_samples_store: Arc<DbPruningSamplesStore>,
    pruning_meta_stores: Arc<RwLock<PruningMetaStores>>,

    ghostdag_manager: DbGhostdagManager,
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
            ghostdag_store: storage.ghostdag_store.clone(),
            relations_stores: storage.relations_stores.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            pruning_meta_stores: storage.pruning_meta_stores.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            headers_selected_tip_store: storage.headers_selected_tip_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            depth_store: storage.depth_store.clone(),
            pruning_samples_store: storage.pruning_samples_store.clone(),

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
            ghostdag_manager,

            is_consensus_exiting,

            level_relations_services: (0..=max_block_level)
                .map(|level| MTRelationsService::new(storage.relations_stores.clone().clone(), level))
                .collect_vec(),
        }
    }

    pub fn import_pruning_points(&self, pruning_points: &[Arc<Header>]) -> PruningImportResult<()> {
        let unique_count = pruning_points.iter().map(|h| h.hash).unique().count();
        if unique_count < pruning_points.len() {
            return Err(PruningImportError::DuplicatedPastPruningPoints(pruning_points.len() - unique_count));
        }
        for (i, header) in pruning_points.iter().enumerate() {
            self.past_pruning_points_store.set(i as u64, header.hash).unwrap();

            if i > 0 {
                let prev_blue_score = pruning_points[i - 1].blue_score;
                // This is a sufficient condition for running expected pruning point algo (v2)
                // over blocks B s.t. pruning point ∈ chain(B) w/o a risk of not converging
                if prev_blue_score >= header.blue_score {
                    return Err(PruningImportError::InconsistentPastPruningPoints(i - 1, i, prev_blue_score, header.blue_score));
                }
                // Store the i-1 pruning point as the last pruning sample from POV of the i'th pruning point.
                // If this data is inconsistent, then blocks above the pruning point will fail the expected
                // pruning point validation performed at are_pruning_points_in_valid_chain
                self.pruning_samples_store.insert(header.hash, pruning_points[i - 1].hash).unwrap();
            }

            if self.headers_store.has(header.hash).unwrap() {
                continue;
            }

            let block_level = calc_block_level(header, self.max_block_level);
            self.headers_store.insert(header.hash, header.clone(), block_level).unwrap();
        }

        let new_pruning_point = pruning_points.last().unwrap().hash;
        info!("Setting {new_pruning_point} as the staging pruning point");

        let mut pruning_point_write = self.pruning_point_store.write();
        let mut batch = WriteBatch::default();
        pruning_point_write.set_batch(&mut batch, new_pruning_point, (pruning_points.len() - 1) as u64).unwrap();
        pruning_point_write.set_retention_checkpoint(&mut batch, new_pruning_point).unwrap();
        pruning_point_write.set_retention_period_root(&mut batch, new_pruning_point).unwrap();
        self.db.write(batch).unwrap();
        drop(pruning_point_write);

        Ok(())
    }

    // Used in apply and validate
    fn estimate_proof_unique_size(&self, proof: &PruningPointProof) -> usize {
        let approx_history_size = proof[0][0].daa_score;
        let approx_unique_full_levels = f64::log2(approx_history_size as f64 / self.pruning_proof_m as f64).max(0f64) as usize;
        proof.iter().map(|l| l.len()).sum::<usize>().min((approx_unique_full_levels + 1) * self.pruning_proof_m as usize)
    }

    // Used in build and validate
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

    /// Returns the k + 1 chain blocks below this hash (inclusive). If data is missing
    /// the search is halted and a partial chain is returned.
    ///
    /// The returned hashes are guaranteed to have GHOSTDAG data
    pub(crate) fn get_ghostdag_chain_k_depth(&self, hash: Hash, ghostdag_k: KType) -> Vec<Hash> {
        let mut hashes = Vec::with_capacity(ghostdag_k as usize + 1);
        let mut current = hash;
        for _ in 0..=ghostdag_k {
            hashes.push(current);
            let Some(parent) = self.ghostdag_store.get_selected_parent(current).unwrap_option() else {
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
        let mut anticone = self.ghostdag_manager.sort_blocks(anticone);
        anticone.insert(0, pruning_point);

        let mut daa_window_blocks = BlockHashMap::new();
        let mut ghostdag_blocks = BlockHashMap::new();

        let ghostdag_k = self.ghostdag_k;

        // PRUNE SAFETY: called either via consensus under the prune guard or by the pruning processor (hence no pruning in parallel)

        for anticone_block in anticone.iter().copied() {
            let ghostdag = self.ghostdag_store.get_data(anticone_block).unwrap();
            let window = self.window_manager.block_window(&ghostdag, WindowType::DifficultyWindow).unwrap();
            let cover = self.window_manager.consecutive_cover_for_window(ghostdag, &window);

            for hash in cover {
                if let Entry::Vacant(e) = daa_window_blocks.entry(hash) {
                    e.insert(TrustedHeader {
                        header: self.headers_store.get_header(hash).unwrap(),
                        ghostdag: (&*self.ghostdag_store.get_data(hash).unwrap()).into(),
                    });
                }
            }

            let ghostdag_chain = self.get_ghostdag_chain_k_depth(anticone_block, ghostdag_k);
            for hash in ghostdag_chain {
                if let Entry::Vacant(e) = ghostdag_blocks.entry(hash) {
                    let ghostdag = self.ghostdag_store.get_data(hash).unwrap();
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
                let ghostdag = (&*self.ghostdag_store.get_data(current).unwrap()).into();
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
        info!(
            "Built headers proof with overall {} headers ({} unique)",
            proof.iter().map(|l| l.len()).sum::<usize>(),
            proof.iter().flatten().unique_by(|h| h.hash).count()
        );
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
