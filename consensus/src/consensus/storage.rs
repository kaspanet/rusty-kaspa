use crate::{
    config::Config,
    model::stores::{
        acceptance_data::DbAcceptanceDataStore,
        block_transactions::DbBlockTransactionsStore,
        block_window_cache::BlockWindowCacheStore,
        daa::DbDaaStore,
        depth::DbDepthStore,
        ghostdag::{CompactGhostdagData, DbGhostdagStore},
        headers::{CompactHeaderData, DbHeadersStore},
        headers_selected_tip::DbHeadersSelectedTipStore,
        past_pruning_points::DbPastPruningPointsStore,
        pruning::DbPruningStore,
        pruning_utxoset::PruningUtxosetStores,
        reachability::{DbReachabilityStore, ReachabilityData},
        relations::DbRelationsStore,
        selected_chain::DbSelectedChainStore,
        statuses::DbStatusesStore,
        tips::DbTipsStore,
        utxo_diffs::DbUtxoDiffsStore,
        utxo_multisets::DbUtxoMultisetsStore,
        virtual_state::{LkgVirtualState, VirtualStores},
        DB,
    },
    processes::{ghostdag::ordering::SortableBlock, reachability::inquirer as reachability, relations},
};

use super::cache_policy_builder::CachePolicyBuilder as PolicyBuilder;
use itertools::Itertools;
use kaspa_consensus_core::{blockstatus::BlockStatus, BlockHashSet};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use parking_lot::RwLock;
use std::{mem::size_of, ops::DerefMut, sync::Arc};

pub struct ConsensusStorage {
    // DB
    db: Arc<DB>,

    // Locked stores
    pub statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    pub reachability_store: Arc<RwLock<DbReachabilityStore>>,
    pub reachability_relations_store: Arc<RwLock<DbRelationsStore>>,
    pub pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    pub body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub pruning_utxoset_stores: Arc<RwLock<PruningUtxosetStores>>,
    pub virtual_stores: Arc<RwLock<VirtualStores>>,
    pub selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,

    // Append-only stores
    pub ghostdag_store: Arc<DbGhostdagStore>,
    pub headers_store: Arc<DbHeadersStore>,
    pub block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub daa_excluded_store: Arc<DbDaaStore>,
    pub depth_store: Arc<DbDepthStore>,

    // Utxo-related stores
    pub utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    pub utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub acceptance_data_store: Arc<DbAcceptanceDataStore>,

    // Block window caches
    pub block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,

    // "Last Known Good" caches
    /// The "last known good" virtual state. To be used by any logic which does not want to wait
    /// for a possible virtual state write to complete but can rather settle with the last known state
    pub lkg_virtual_state: LkgVirtualState,
}

impl ConsensusStorage {
    pub fn new(db: Arc<DB>, config: Arc<Config>) -> Arc<Self> {
        let scale_factor = config.ram_scale;
        let scaled = |s| (s as f64 * scale_factor) as usize;

        let params = &config.params;
        let perf_params = &config.perf;

        // Lower and upper bounds
        let pruning_depth = params.pruning_depth as usize;
        let pruning_size_for_caches = (params.pruning_depth + params.finality_depth) as usize; // Upper bound for any block/header related data
        let level_lower_bound = 2 * params.pruning_proof_m as usize; // Number of items lower bound for level-related caches

        // Budgets in bytes. All byte budgets overall sum up to ~1GB of memory (which obviously takes more low level alloc space)
        let daa_excluded_budget = scaled(30_000_000);
        let statuses_budget = scaled(30_000_000);
        let reachability_data_budget = scaled(200_000_000);
        let reachability_sets_budget = scaled(200_000_000); // x 2 for tree children and future covering set
        let ghostdag_compact_budget = scaled(15_000_000);
        let headers_compact_budget = scaled(5_000_000);
        let parents_budget = scaled(80_000_000); // x 3 for reachability and levels
        let children_budget = scaled(20_000_000); // x 3 for reachability and levels
        let ghostdag_budget = scaled(80_000_000); // x 2 for levels
        let headers_budget = scaled(80_000_000);
        let transactions_budget = scaled(40_000_000);
        let utxo_diffs_budget = scaled(40_000_000);
        let block_window_budget = scaled(200_000_000); // x 2 for difficulty and median time
        let acceptance_data_budget = scaled(40_000_000);

        // Unit sizes in bytes
        let daa_excluded_bytes = size_of::<Hash>() + size_of::<BlockHashSet>(); // Expected empty sets
        let status_bytes = size_of::<Hash>() + size_of::<BlockStatus>();
        let reachability_data_bytes = size_of::<Hash>() + size_of::<ReachabilityData>();
        let ghostdag_compact_bytes = size_of::<Hash>() + size_of::<CompactGhostdagData>();
        let headers_compact_bytes = size_of::<Hash>() + size_of::<CompactHeaderData>();
        let difficulty_window_bytes = params.difficulty_window_size(0) * size_of::<SortableBlock>();
        let median_window_bytes = params.past_median_time_window_size(0) * size_of::<SortableBlock>();

        // Cache policy builders
        let daa_excluded_builder =
            PolicyBuilder::new().max_items(pruning_depth).bytes_budget(daa_excluded_budget).unit_bytes(daa_excluded_bytes).untracked(); // Required only above the pruning point
        let statuses_builder =
            PolicyBuilder::new().max_items(pruning_size_for_caches).bytes_budget(statuses_budget).unit_bytes(status_bytes).untracked();
        let reachability_data_builder = PolicyBuilder::new()
            .max_items(pruning_size_for_caches)
            .bytes_budget(reachability_data_budget)
            .unit_bytes(reachability_data_bytes)
            .untracked();
        let ghostdag_compact_builder = PolicyBuilder::new()
            .max_items(pruning_size_for_caches)
            .bytes_budget(ghostdag_compact_budget)
            .unit_bytes(ghostdag_compact_bytes)
            .min_items(level_lower_bound)
            .untracked();
        let headers_compact_builder = PolicyBuilder::new()
            .max_items(pruning_size_for_caches)
            .bytes_budget(headers_compact_budget)
            .unit_bytes(headers_compact_bytes)
            .untracked();
        let parents_builder = PolicyBuilder::new()
            .bytes_budget(parents_budget)
            .unit_bytes(size_of::<Hash>())
            .min_items(level_lower_bound)
            .tracked_units();
        let children_builder = PolicyBuilder::new()
            .bytes_budget(children_budget)
            .unit_bytes(size_of::<Hash>())
            .min_items(level_lower_bound)
            .tracked_units();
        let reachability_sets_builder =
            PolicyBuilder::new().bytes_budget(reachability_sets_budget).unit_bytes(size_of::<Hash>()).tracked_units();
        let difficulty_window_builder = PolicyBuilder::new()
            .max_items(perf_params.block_window_cache_size)
            .bytes_budget(block_window_budget)
            .unit_bytes(difficulty_window_bytes)
            .untracked();
        let median_window_builder = PolicyBuilder::new()
            .max_items(perf_params.block_window_cache_size)
            .bytes_budget(block_window_budget)
            .unit_bytes(median_window_bytes)
            .untracked();
        let ghostdag_builder = PolicyBuilder::new().bytes_budget(ghostdag_budget).min_items(level_lower_bound).tracked_bytes();
        let headers_builder = PolicyBuilder::new().bytes_budget(headers_budget).tracked_bytes();
        let utxo_diffs_builder = PolicyBuilder::new().bytes_budget(utxo_diffs_budget).tracked_bytes();
        let block_data_builder = PolicyBuilder::new().max_items(perf_params.block_data_cache_size).untracked();
        let header_data_builder = PolicyBuilder::new().max_items(perf_params.header_data_cache_size).untracked();
        let utxo_set_builder = PolicyBuilder::new().max_items(perf_params.utxo_set_cache_size).untracked();
        let transactions_builder = PolicyBuilder::new().bytes_budget(transactions_budget).tracked_bytes();
        let acceptance_data_builder = PolicyBuilder::new().bytes_budget(acceptance_data_budget).tracked_bytes();
        let past_pruning_points_builder = PolicyBuilder::new().max_items(1024).untracked();

        // TODO: consider tracking UtxoDiff byte sizes more accurately including the exact size of ScriptPublicKey

        // Headers
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), statuses_builder.build())));
        let relations_stores = Arc::new(RwLock::new(
            (0..=params.max_block_level)
                .map(|level| {
                    DbRelationsStore::new(
                        db.clone(),
                        level,
                        parents_builder.downscale(level).build(),
                        children_builder.downscale(level).build(),
                    )
                })
                .collect_vec(),
        ));
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(
            db.clone(),
            reachability_data_builder.build(),
            reachability_sets_builder.build(),
        )));

        let reachability_relations_store = Arc::new(RwLock::new(DbRelationsStore::with_prefix(
            db.clone(),
            DatabaseStorePrefixes::ReachabilityRelations.as_ref(),
            parents_builder.build(),
            children_builder.build(),
        )));

        let ghostdag_store = Arc::new(DbGhostdagStore::new(
            db.clone(),
            0,
            ghostdag_builder.downscale(0).build(),
            ghostdag_compact_builder.downscale(0).build(),
        ));
        let daa_excluded_store = Arc::new(DbDaaStore::new(db.clone(), daa_excluded_builder.build()));
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), headers_builder.build(), headers_compact_builder.build()));
        let depth_store = Arc::new(DbDepthStore::new(db.clone(), header_data_builder.build()));
        let selected_chain_store = Arc::new(RwLock::new(DbSelectedChainStore::new(db.clone(), header_data_builder.build())));

        // Pruning
        let pruning_point_store = Arc::new(RwLock::new(DbPruningStore::new(db.clone())));
        let past_pruning_points_store = Arc::new(DbPastPruningPointsStore::new(db.clone(), past_pruning_points_builder.build()));
        let pruning_utxoset_stores = Arc::new(RwLock::new(PruningUtxosetStores::new(db.clone(), utxo_set_builder.build())));

        // Txs
        let block_transactions_store = Arc::new(DbBlockTransactionsStore::new(db.clone(), transactions_builder.build()));
        let utxo_diffs_store = Arc::new(DbUtxoDiffsStore::new(db.clone(), utxo_diffs_builder.build()));
        let utxo_multisets_store = Arc::new(DbUtxoMultisetsStore::new(db.clone(), block_data_builder.build()));
        let acceptance_data_store = Arc::new(DbAcceptanceDataStore::new(db.clone(), acceptance_data_builder.build()));

        // Tips
        let headers_selected_tip_store = Arc::new(RwLock::new(DbHeadersSelectedTipStore::new(db.clone())));
        let body_tips_store = Arc::new(RwLock::new(DbTipsStore::new(db.clone())));

        // Block windows
        let block_window_cache_for_difficulty = Arc::new(BlockWindowCacheStore::new(difficulty_window_builder.build()));
        let block_window_cache_for_past_median_time = Arc::new(BlockWindowCacheStore::new(median_window_builder.build()));

        // Virtual stores
        let lkg_virtual_state = LkgVirtualState::default();
        let virtual_stores =
            Arc::new(RwLock::new(VirtualStores::new(db.clone(), lkg_virtual_state.clone(), utxo_set_builder.build())));

        // Ensure that reachability stores are initialized
        reachability::init(reachability_store.write().deref_mut()).unwrap();
        relations::init(reachability_relations_store.write().deref_mut());

        Arc::new(Self {
            db,
            statuses_store,
            relations_stores,
            reachability_relations_store,
            reachability_store,
            ghostdag_store,
            pruning_point_store,
            headers_selected_tip_store,
            body_tips_store,
            headers_store,
            block_transactions_store,
            pruning_utxoset_stores,
            virtual_stores,
            selected_chain_store,
            acceptance_data_store,
            past_pruning_points_store,
            daa_excluded_store,
            depth_store,
            utxo_diffs_store,
            utxo_multisets_store,
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            lkg_virtual_state,
        })
    }
}
