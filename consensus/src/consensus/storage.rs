use crate::{
    config::Config,
    model::stores::{
        acceptance_data::DbAcceptanceDataStore,
        block_transactions::DbBlockTransactionsStore,
        block_window_cache::BlockWindowCacheStore,
        daa::DbDaaStore,
        depth::DbDepthStore,
        ghostdag::{DbGhostdagStore, GhostdagData},
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
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use parking_lot::RwLock;
use rand::Rng;
use std::{
    cmp::max,
    mem::size_of,
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub struct ConsensusStorage {
    // DB
    db: Arc<DB>,

    // Locked stores
    pub statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub relations_stores: Arc<[DbRelationsStore]>,
    pub relations_primary_store: DbRelationsStore,
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

        let pruning_size_for_caches = params.pruning_depth + params.finality_depth;

        // Calculate cache sizes which are related to pruning depth
        let daa_excluded_cache_size =
            perf::bounded_cache_size(params.pruning_depth, 100_000_000u64, size_of::<Hash>() + size_of::<BlockHashSet>()); // required only above the pruning point; 100MB budget; expected empty sets
        let statuses_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 100_000_000u64, size_of::<Hash>() + size_of::<BlockStatus>());
        let relations_cache_size = perf::bounded_cache_size(
            pruning_size_for_caches,
            200_000_000u64,
            size_of::<Hash>() + perf::approx_direct_header_parents(params) * size_of::<Hash>(),
        );
        let reachability_relations_cache_size = perf::bounded_cache_size(
            pruning_size_for_caches,
            100_000_000u64,
            size_of::<Hash>() + perf::approx_direct_header_parents(params) * size_of::<Hash>(),
        );
        let reachability_cache_size = perf::bounded_cache_size(
            pruning_size_for_caches,
            200_000_000u64,
            size_of::<ReachabilityData>() + perf::approx_mergeset_size(params) * size_of::<Hash>(),
        );
        let ghostdag_cache_size = perf::bounded_cache_size(
            pruning_size_for_caches,
            200_000_000u64,
            size_of::<GhostdagData>() + perf::approx_mergeset_size(params) * size_of::<Hash>() * 2,
        );

        // Add stochastic noise to cache sizes to avoid predictable and equal sizes across all network nodes
        let noise = |size| size + rand::thread_rng().gen_range(0..16);

        // Headers
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), noise(statuses_cache_size))));
        let relations_stores: Arc<[_]> = (0..=params.max_block_level)
            .map(|level| {
                let cache_size = max(relations_cache_size.checked_shr(level as u32).unwrap_or(0), 2 * params.pruning_proof_m);
                DbRelationsStore::new(db.clone(), level, noise(cache_size))
            })
            .collect_vec()
            .into();
        let relations_primary_store = relations_stores[0].clone();
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), noise(reachability_cache_size))));

        let reachability_relations_store = Arc::new(RwLock::new(DbRelationsStore::with_prefix(
            db.clone(),
            DatabaseStorePrefixes::ReachabilityRelations.as_ref(),
            noise(reachability_relations_cache_size),
        )));
        let ghostdag_stores = Arc::new(
            (0..=params.max_block_level)
                .map(|level| {
                    let cache_size = max(ghostdag_cache_size.checked_shr(level as u32).unwrap_or(0), 2 * params.pruning_proof_m);
                    Arc::new(DbGhostdagStore::new(db.clone(), level, noise(cache_size)))
                })
                .collect_vec(),
        );
        let ghostdag_primary_store = ghostdag_stores[0].clone();
        let daa_excluded_store = Arc::new(DbDaaStore::new(db.clone(), noise(daa_excluded_cache_size)));
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), noise(perf_params.headers_cache_size)));
        let depth_store = Arc::new(DbDepthStore::new(db.clone(), noise(perf_params.header_data_cache_size)));
        let selected_chain_store =
            Arc::new(RwLock::new(DbSelectedChainStore::new(db.clone(), noise(perf_params.header_data_cache_size))));

        // Pruning
        let pruning_point_store = Arc::new(RwLock::new(DbPruningStore::new(db.clone())));
        let past_pruning_points_store = Arc::new(DbPastPruningPointsStore::new(db.clone(), 4));
        let pruning_utxoset_stores =
            Arc::new(RwLock::new(PruningUtxosetStores::new(db.clone(), noise(perf_params.utxo_set_cache_size))));

        // Txs
        let block_transactions_store = Arc::new(DbBlockTransactionsStore::new(db.clone(), noise(perf_params.block_data_cache_size)));
        let utxo_diffs_store = Arc::new(DbUtxoDiffsStore::new(db.clone(), noise(perf_params.block_data_cache_size)));
        let utxo_multisets_store = Arc::new(DbUtxoMultisetsStore::new(db.clone(), noise(perf_params.block_data_cache_size)));
        let acceptance_data_store = Arc::new(DbAcceptanceDataStore::new(db.clone(), noise(perf_params.block_data_cache_size)));

        // Tips
        let headers_selected_tip_store = Arc::new(RwLock::new(DbHeadersSelectedTipStore::new(db.clone())));
        let body_tips_store = Arc::new(RwLock::new(DbTipsStore::new(db.clone())));

        // Block windows
        let block_window_cache_for_difficulty = Arc::new(BlockWindowCacheStore::new(noise(perf_params.block_window_cache_size)));
        let block_window_cache_for_past_median_time = Arc::new(BlockWindowCacheStore::new(noise(perf_params.block_window_cache_size)));

        // Virtual stores
        let virtual_stores = Arc::new(RwLock::new(VirtualStores::new(db.clone(), noise(perf_params.utxo_set_cache_size))));

        // Ensure that reachability stores are initialized
        reachability::init(reachability_store.write().deref_mut()).unwrap();
        relations::init(&mut reachability_relations_store.write().deref());

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
            relations_primary_store,
        })
    }
}
