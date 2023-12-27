use crate::{
    config::Config,
    model::stores::{
        acceptance_data::DbAcceptanceDataStore,
        block_transactions::DbBlockTransactionsStore,
        block_window_cache::BlockWindowCacheStore,
        daa::DbDaaStore,
        depth::DbDepthStore,
        ghostdag::{CompactGhostdagData, DbGhostdagStore},
        headers::DbHeadersStore,
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
        virtual_state::VirtualStores,
        DB,
    },
    processes::{reachability::inquirer as reachability, relations},
};

use itertools::Itertools;

use kaspa_consensus_core::{blockstatus::BlockStatus, config::constants::perf, BlockHashSet};
use kaspa_database::{prelude::CachePolicy, registry::DatabaseStorePrefixes};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemMode;
use parking_lot::RwLock;
use rand::Rng;
use std::{cmp::max, mem::size_of, ops::DerefMut, sync::Arc};

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
    pub ghostdag_stores: Arc<Vec<Arc<DbGhostdagStore>>>,
    pub ghostdag_primary_store: Arc<DbGhostdagStore>,
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
}

impl ConsensusStorage {
    pub fn new(db: Arc<DB>, config: Arc<Config>) -> Arc<Self> {
        let params = &config.params;
        let perf_params = &config.perf;

        let pruning_size_for_caches = (params.pruning_depth + params.finality_depth) as usize;

        // Calculate cache sizes which are related to pruning depth
        let daa_excluded_cache_size =
            perf::bounded_cache_size(params.pruning_depth as usize, 30_000_000, size_of::<Hash>() + size_of::<BlockHashSet>()); // required only above the pruning point; expected empty sets
        let statuses_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 30_000_000, size_of::<Hash>() + size_of::<BlockStatus>());
        let reachability_data_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 20_000_000, size_of::<Hash>() + size_of::<ReachabilityData>());
        let reachability_sets_cache_size = perf::bounded_cache_size(pruning_size_for_caches, 20_000_000, size_of::<Hash>());
        let ghostdag_compact_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 15_000_000, size_of::<Hash>() + size_of::<CompactGhostdagData>());

        // Cache sizes which are tracked per unit
        let parents_cache_size = 40_000_000 / size_of::<Hash>();
        let children_cache_size = 5_000_000 / size_of::<Hash>(); // Children relations are hardly used in consensus processing so the cache can be small
        let reachability_parents_cache_size = 40_000_000 / size_of::<Hash>();
        let reachability_children_cache_size = 5_000_000 / size_of::<Hash>();
        let transactions_cache_size = 40_000usize; // Tracked units are txs

        // Cache sizes represented and tracked as bytes
        let ghostdag_cache_bytes = 80_000_000usize;
        let headers_cache_bytes = 80_000_000usize;
        let utxo_diffs_cache_bytes = 40_000_000usize;

        // Number of units lower bound for level-related caches
        let unit_lower_bound = 2 * params.pruning_proof_m as usize;

        // Add stochastic noise to cache sizes to avoid predictable and equal sizes across all network nodes
        let noise = |size| size + rand::thread_rng().gen_range(0..16);

        // Headers
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), CachePolicy::Unit(noise(statuses_cache_size)))));
        let relations_stores = Arc::new(RwLock::new(
            (0..=params.max_block_level)
                .map(|level| {
                    // = size / 2^level
                    let parents_level_size = parents_cache_size.checked_shr(level as u32).unwrap_or(0);
                    let parents_cache_policy = CachePolicy::LowerBoundedTracked {
                        max_size: noise(parents_level_size),
                        min_items: noise(unit_lower_bound),
                        mem_mode: MemMode::Units,
                    };
                    let children_level_size = children_cache_size.checked_shr(level as u32).unwrap_or(0);
                    let children_cache_policy = CachePolicy::LowerBoundedTracked {
                        max_size: noise(children_level_size),
                        min_items: noise(unit_lower_bound),
                        mem_mode: MemMode::Units,
                    };
                    DbRelationsStore::new(db.clone(), level, parents_cache_policy, children_cache_policy)
                })
                .collect_vec(),
        ));
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(
            db.clone(),
            CachePolicy::Unit(noise(reachability_data_cache_size)),
            CachePolicy::Tracked(noise(reachability_sets_cache_size), MemMode::Units),
        )));

        let reachability_relations_store = Arc::new(RwLock::new(DbRelationsStore::with_prefix(
            db.clone(),
            DatabaseStorePrefixes::ReachabilityRelations.as_ref(),
            CachePolicy::Tracked(noise(reachability_parents_cache_size), MemMode::Units),
            CachePolicy::Tracked(noise(reachability_children_cache_size), MemMode::Units),
        )));

        let ghostdag_stores = Arc::new(
            (0..=params.max_block_level)
                .map(|level| {
                    // = size / 2^level
                    let level_cache_bytes = ghostdag_cache_bytes.checked_shr(level as u32).unwrap_or(0);
                    let cache_policy = CachePolicy::LowerBoundedTracked {
                        max_size: noise(level_cache_bytes),
                        min_items: noise(unit_lower_bound),
                        mem_mode: MemMode::Bytes,
                    };
                    let compact_cache_size = max(ghostdag_compact_cache_size.checked_shr(level as u32).unwrap_or(0), unit_lower_bound);
                    Arc::new(DbGhostdagStore::new(db.clone(), level, cache_policy, CachePolicy::Unit(noise(compact_cache_size))))
                })
                .collect_vec(),
        );
        let ghostdag_primary_store = ghostdag_stores[0].clone();
        let daa_excluded_store = Arc::new(DbDaaStore::new(db.clone(), CachePolicy::Unit(noise(daa_excluded_cache_size))));
        let headers_store = Arc::new(DbHeadersStore::new(
            db.clone(),
            CachePolicy::Tracked(noise(headers_cache_bytes), MemMode::Bytes),
            CachePolicy::Unit(noise((3600 * params.bps() as usize).max(perf_params.header_data_cache_size))),
        ));
        let depth_store = Arc::new(DbDepthStore::new(db.clone(), CachePolicy::Unit(noise(perf_params.header_data_cache_size))));
        let selected_chain_store =
            Arc::new(RwLock::new(DbSelectedChainStore::new(db.clone(), CachePolicy::Unit(noise(perf_params.header_data_cache_size)))));

        // Pruning
        let pruning_point_store = Arc::new(RwLock::new(DbPruningStore::new(db.clone())));
        let past_pruning_points_store = Arc::new(DbPastPruningPointsStore::new(db.clone(), CachePolicy::Unit(1024)));
        let pruning_utxoset_stores =
            Arc::new(RwLock::new(PruningUtxosetStores::new(db.clone(), CachePolicy::Unit(noise(perf_params.utxo_set_cache_size)))));

        // Txs
        let block_transactions_store =
            Arc::new(DbBlockTransactionsStore::new(db.clone(), CachePolicy::Tracked(noise(transactions_cache_size), MemMode::Units)));
        let utxo_diffs_store =
            Arc::new(DbUtxoDiffsStore::new(db.clone(), CachePolicy::Tracked(noise(utxo_diffs_cache_bytes), MemMode::Bytes)));
        let utxo_multisets_store =
            Arc::new(DbUtxoMultisetsStore::new(db.clone(), CachePolicy::Unit(noise(perf_params.block_data_cache_size))));
        let acceptance_data_store =
            Arc::new(DbAcceptanceDataStore::new(db.clone(), CachePolicy::Unit(noise(perf_params.block_data_cache_size))));

        // Tips
        let headers_selected_tip_store = Arc::new(RwLock::new(DbHeadersSelectedTipStore::new(db.clone())));
        let body_tips_store = Arc::new(RwLock::new(DbTipsStore::new(db.clone())));

        // Block windows
        let block_window_cache_for_difficulty =
            Arc::new(BlockWindowCacheStore::new(CachePolicy::Unit(noise(perf_params.block_window_cache_size))));
        let block_window_cache_for_past_median_time =
            Arc::new(BlockWindowCacheStore::new(CachePolicy::Unit(noise(perf_params.block_window_cache_size))));

        // Virtual stores
        let virtual_stores =
            Arc::new(RwLock::new(VirtualStores::new(db.clone(), CachePolicy::Unit(noise(perf_params.utxo_set_cache_size)))));

        // Ensure that reachability stores are initialized
        reachability::init(reachability_store.write().deref_mut()).unwrap();
        relations::init(reachability_relations_store.write().deref_mut());

        Arc::new(Self {
            db,
            statuses_store,
            relations_stores,
            reachability_relations_store,
            reachability_store,
            ghostdag_stores,
            ghostdag_primary_store,
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
        })
    }
}
