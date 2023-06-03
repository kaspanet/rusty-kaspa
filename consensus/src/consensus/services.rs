use super::storage::ConsensusStorage;
use crate::{
    config::Config,
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService, statuses::MTStatusesService},
        stores::{
            block_window_cache::BlockWindowCacheStore, depth::DbDepthStore, ghostdag::DbGhostdagStore, headers::DbHeadersStore,
            headers_selected_tip::DbHeadersSelectedTipStore, past_pruning_points::DbPastPruningPointsStore, pruning::DbPruningStore,
            reachability::DbReachabilityStore, relations::DbRelationsStore, selected_chain::DbSelectedChainStore,
            statuses::DbStatusesStore, DB,
        },
    },
    processes::{
        block_depth::BlockDepthManager, coinbase::CoinbaseManager, difficulty::DifficultyManager, ghostdag::protocol::GhostdagManager,
        mass::MassCalculator, parents_builder::ParentsManager, past_median_time::PastMedianTimeManager, pruning::PruningPointManager,
        pruning_proof::PruningProofManager, sync::SyncManager, transaction_validator::TransactionValidator,
        traversal_manager::DagTraversalManager,
    },
};

use itertools::Itertools;
use std::{cmp::min, sync::Arc};

pub type DbGhostdagManager =
    GhostdagManager<DbGhostdagStore, MTRelationsService<DbRelationsStore>, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>;

pub type DbDagTraversalManager =
    DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>;

pub type DbPastMedianTimeManager = PastMedianTimeManager<
    DbHeadersStore,
    DbGhostdagStore,
    BlockWindowCacheStore,
    DbReachabilityStore,
    MTRelationsService<DbRelationsStore>,
>;

pub type DbSyncManager = SyncManager<
    MTRelationsService<DbRelationsStore>,
    DbReachabilityStore,
    DbGhostdagStore,
    DbSelectedChainStore,
    DbHeadersSelectedTipStore,
    DbPruningStore,
    DbStatusesStore,
    BlockWindowCacheStore,
>;

pub type DbDifficultyManager = DifficultyManager<DbHeadersStore>;
pub type DbPruningPointManager = PruningPointManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>;
pub type DbBlockDepthManager = BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>;
pub type DbParentsManager = ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>;

pub struct ConsensusServices {
    // Underlying storage
    storage: Arc<ConsensusStorage>,

    // Services and managers
    pub statuses_service: MTStatusesService<DbStatusesStore>,
    pub relations_service: MTRelationsService<DbRelationsStore>,
    pub reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub difficulty_manager: DbDifficultyManager,
    pub dag_traversal_manager: DbDagTraversalManager,
    pub ghostdag_managers: Arc<Vec<DbGhostdagManager>>,
    pub ghostdag_primary_manager: DbGhostdagManager,
    pub past_median_time_manager: DbPastMedianTimeManager,
    pub coinbase_manager: CoinbaseManager,
    pub pruning_point_manager: DbPruningPointManager,
    pub pruning_proof_manager: Arc<PruningProofManager>,
    pub parents_manager: DbParentsManager,
    pub sync_manager: DbSyncManager,
    pub depth_manager: DbBlockDepthManager,
    pub mass_calculator: MassCalculator,
    pub transaction_validator: TransactionValidator,
}

impl ConsensusServices {
    pub fn new(db: Arc<DB>, storage: Arc<ConsensusStorage>, config: Arc<Config>) -> Arc<Self> {
        let params = &config.params;

        let statuses_service = MTStatusesService::new(storage.statuses_store.clone());
        let relations_services =
            (0..=params.max_block_level).map(|level| MTRelationsService::new(storage.relations_stores.clone(), level)).collect_vec();
        let relations_service = relations_services[0].clone();
        let reachability_service = MTReachabilityService::new(storage.reachability_store.clone());
        let dag_traversal_manager = DagTraversalManager::new(
            params.genesis.hash,
            storage.ghostdag_primary_store.clone(),
            relations_service.clone(),
            storage.block_window_cache_for_difficulty.clone(),
            storage.block_window_cache_for_past_median_time.clone(),
            params.difficulty_window_size,
            (2 * params.timestamp_deviation_tolerance - 1) as usize, // TODO: incorporate target_time_per_block to this calculation
            reachability_service.clone(),
        );
        let past_median_time_manager = PastMedianTimeManager::new(
            storage.headers_store.clone(),
            dag_traversal_manager.clone(),
            params.timestamp_deviation_tolerance as usize,
            params.genesis.timestamp,
        );
        let difficulty_manager = DifficultyManager::new(
            storage.headers_store.clone(),
            params.genesis.bits,
            params.difficulty_window_size,
            params.target_time_per_block,
        );
        let depth_manager = BlockDepthManager::new(
            params.merge_depth,
            params.finality_depth,
            params.genesis.hash,
            storage.depth_store.clone(),
            reachability_service.clone(),
            storage.ghostdag_primary_store.clone(),
        );
        let ghostdag_managers = Arc::new(
            storage
                .ghostdag_stores
                .iter()
                .cloned()
                .enumerate()
                .map(|(level, ghostdag_store)| {
                    GhostdagManager::new(
                        params.genesis.hash,
                        params.ghostdag_k,
                        ghostdag_store,
                        relations_services[level].clone(),
                        storage.headers_store.clone(),
                        reachability_service.clone(),
                    )
                })
                .collect_vec(),
        );
        let ghostdag_primary_manager = ghostdag_managers[0].clone();

        let coinbase_manager = CoinbaseManager::new(
            params.coinbase_payload_script_public_key_max_len,
            params.max_coinbase_payload_len,
            params.deflationary_phase_daa_score,
            params.pre_deflationary_phase_base_subsidy,
        );

        let mass_calculator =
            MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op);

        let transaction_validator = TransactionValidator::new(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
        );

        let pruning_point_manager = PruningPointManager::new(
            params.pruning_depth,
            params.finality_depth,
            params.genesis.hash,
            reachability_service.clone(),
            storage.ghostdag_primary_store.clone(),
            storage.headers_store.clone(),
            storage.past_pruning_points_store.clone(),
        );

        let parents_manager = ParentsManager::new(
            params.max_block_level,
            params.genesis.hash,
            storage.headers_store.clone(),
            reachability_service.clone(),
            relations_service.clone(),
        );

        let pruning_proof_manager = Arc::new(PruningProofManager::new(
            db,
            &storage,
            parents_manager.clone(),
            reachability_service.clone(),
            ghostdag_managers.clone(),
            dag_traversal_manager.clone(),
            params.max_block_level,
            params.genesis.hash,
            params.pruning_proof_m,
            min(params.anticone_finalization_depth(), params.pruning_depth),
            params.difficulty_window_size,
            params.ghostdag_k,
        ));

        let sync_manager = SyncManager::new(
            params.mergeset_size_limit as usize,
            reachability_service.clone(),
            dag_traversal_manager.clone(),
            storage.ghostdag_primary_store.clone(),
            storage.selected_chain_store.clone(),
            storage.headers_selected_tip_store.clone(),
            storage.pruning_point_store.clone(),
            storage.statuses_store.clone(),
        );

        Arc::new(Self {
            storage,
            statuses_service,
            relations_service,
            reachability_service,
            difficulty_manager,
            dag_traversal_manager,
            ghostdag_managers,
            ghostdag_primary_manager,
            past_median_time_manager,
            coinbase_manager,
            pruning_point_manager,
            pruning_proof_manager,
            parents_manager,
            sync_manager,
            depth_manager,
            mass_calculator,
            transaction_validator,
        })
    }
}
