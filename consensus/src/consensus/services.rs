use super::storage::ConsensusStorage;
use crate::{
    config::Config,
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService, statuses::MTStatusesService},
        stores::{
            block_window_cache::BlockWindowCacheStore, daa::DbDaaStore, depth::DbDepthStore, ghostdag::DbGhostdagStore,
            headers::DbHeadersStore, headers_selected_tip::DbHeadersSelectedTipStore, past_pruning_points::DbPastPruningPointsStore,
            pruning::DbPruningStore, reachability::DbReachabilityStore, relations::DbRelationsStore,
            selected_chain::DbSelectedChainStore, statuses::DbStatusesStore, DB,
        },
    },
    processes::{
        block_depth::BlockDepthManager, coinbase::CoinbaseManager, ghostdag::protocol::GhostdagManager, mass::MassCalculator,
        parents_builder::ParentsManager, pruning::PruningPointManager, pruning_proof::PruningProofManager, sync::SyncManager,
        transaction_validator::TransactionValidator, traversal_manager::DagTraversalManager, window::DualWindowManager,
    },
};

use itertools::Itertools;
use kaspa_txscript::caches::TxScriptCacheCounters;
use std::sync::Arc;

pub type DbGhostdagManager =
    GhostdagManager<DbGhostdagStore, MTRelationsService<DbRelationsStore>, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>;

pub type DbDagTraversalManager = DagTraversalManager<DbGhostdagStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>;

pub type DbWindowManager = DualWindowManager<DbGhostdagStore, BlockWindowCacheStore, DbHeadersStore, DbDaaStore>;

pub type DbSyncManager = SyncManager<
    MTRelationsService<DbRelationsStore>,
    DbReachabilityStore,
    DbGhostdagStore,
    DbSelectedChainStore,
    DbHeadersSelectedTipStore,
    DbPruningStore,
    DbStatusesStore,
>;

pub type DbPruningPointManager =
    PruningPointManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore, DbHeadersSelectedTipStore>;
pub type DbBlockDepthManager = BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>;
pub type DbParentsManager = ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>;

pub struct ConsensusServices {
    // Underlying storage
    storage: Arc<ConsensusStorage>,

    // Services and managers
    pub statuses_service: MTStatusesService<DbStatusesStore>,
    pub relations_service: MTRelationsService<DbRelationsStore>,
    pub reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub window_manager: DbWindowManager,
    pub dag_traversal_manager: DbDagTraversalManager,
    pub ghostdag_managers: Arc<Vec<DbGhostdagManager>>,
    pub ghostdag_primary_manager: DbGhostdagManager,
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
    pub fn new(
        db: Arc<DB>,
        storage: Arc<ConsensusStorage>,
        config: Arc<Config>,
        tx_script_cache_counters: Arc<TxScriptCacheCounters>,
    ) -> Arc<Self> {
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
            reachability_service.clone(),
        );
        let window_manager = DualWindowManager::new(
            &params.genesis,
            storage.ghostdag_primary_store.clone(),
            storage.headers_store.clone(),
            storage.daa_excluded_store.clone(),
            storage.block_window_cache_for_difficulty.clone(),
            storage.block_window_cache_for_past_median_time.clone(),
            params.max_difficulty_target,
            params.target_time_per_block,
            params.sampling_activation_daa_score,
            params.legacy_difficulty_window_size,
            params.sampled_difficulty_window_size,
            params.min_difficulty_window_len,
            params.difficulty_sample_rate,
            params.legacy_past_median_time_window_size(),
            params.sampled_past_median_time_window_size(),
            params.past_median_time_sample_rate,
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
            params.target_time_per_block,
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
            tx_script_cache_counters,
        );

        let pruning_point_manager = PruningPointManager::new(
            params.pruning_depth,
            params.finality_depth,
            params.genesis.hash,
            reachability_service.clone(),
            storage.ghostdag_primary_store.clone(),
            storage.headers_store.clone(),
            storage.past_pruning_points_store.clone(),
            storage.headers_selected_tip_store.clone(),
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
            window_manager.clone(),
            params.max_block_level,
            params.genesis.hash,
            params.pruning_proof_m,
            params.anticone_finalization_depth(),
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
            window_manager,
            dag_traversal_manager,
            ghostdag_managers,
            ghostdag_primary_manager,
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
