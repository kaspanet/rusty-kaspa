use crate::{
    config::Config,
    model::stores::{
        acceptance_data::DbAcceptanceDataStore,
        block_transactions::DbBlockTransactionsStore,
        block_window_cache::BlockWindowCacheStore,
        daa::DbDaaStore,
        depth::DbDepthStore,
        ghostdag::{CompactGhostdagData, DbGhostdagStore, GhostdagData},
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

use kaspa_consensus_core::{blockstatus::BlockStatus, config::constants::perf, BlockHashSet, KType};
use kaspa_database::{prelude::CachePolicy, registry::DatabaseStorePrefixes};
use kaspa_hashes::Hash;
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
            perf::bounded_cache_size(params.pruning_depth as usize, 40_000_000, size_of::<Hash>() + size_of::<BlockHashSet>()); // required only above the pruning point; expected empty sets
        let statuses_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 40_000_000, size_of::<Hash>() + size_of::<BlockStatus>());
        let reachability_data_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 40_000_000, size_of::<Hash>() + size_of::<ReachabilityData>());
        let reachability_sets_cache_size = perf::bounded_cache_size(pruning_size_for_caches, 20_000_000, size_of::<Hash>());
        let ghostdag_compact_cache_size =
            perf::bounded_cache_size(pruning_size_for_caches, 20_000_000, size_of::<Hash>() + size_of::<CompactGhostdagData>());

        // Cache sizes which are tracked per unit
        let relations_cache_size = 50_000_000 / size_of::<Hash>();
        let relations_children_cache_size = 5_000_000 / size_of::<Hash>();
        let reachability_relations_cache_size = 50_000_000 / size_of::<Hash>();
        let reachability_relations_children_cache_size = 5_000_000 / size_of::<Hash>();
        let transactions_cache_size = 2000usize; // Tracked units are txs

        // Cache sizes represented and tracked as bytes
        // TODO: unit approx for noise magnitude + higher block levels lower bound
        let ghostdag_cache_bytes = 100_000_000usize;
        let headers_cache_bytes = 100_000_000usize;
        let utxo_diffs_cache_bytes = 50_000_000usize;

        // Add stochastic noise to cache sizes to avoid predictable and equal sizes across all network nodes
        let noise = |size| size + rand::thread_rng().gen_range(0..16);

        // Headers
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), CachePolicy::Unit(noise(statuses_cache_size)))));
        let relations_stores = Arc::new(RwLock::new(
            (0..=params.max_block_level)
                .map(|level| {
                    // = size / 2^level
                    let level_normalized_cache_size = relations_cache_size.checked_shr(level as u32).unwrap_or(0);
                    let cache_policy =
                        if level_normalized_cache_size > 2 * params.pruning_proof_m as usize * params.max_block_parents as usize {
                            CachePolicy::Tracked(noise(level_normalized_cache_size))
                        } else {
                            CachePolicy::Unit(noise(2 * params.pruning_proof_m as usize))
                        };
                    let level_normalized_children_cache_size = relations_children_cache_size.checked_shr(level as u32).unwrap_or(0);
                    let children_cache_policy = if level_normalized_children_cache_size
                        > 2 * params.pruning_proof_m as usize * params.max_block_parents as usize
                    {
                        CachePolicy::Tracked(noise(level_normalized_children_cache_size))
                    } else {
                        CachePolicy::Unit(noise(2 * params.pruning_proof_m as usize))
                    };
                    DbRelationsStore::new(db.clone(), level, cache_policy, children_cache_policy)
                })
                .collect_vec(),
        ));
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(
            db.clone(),
            CachePolicy::Unit(noise(reachability_data_cache_size)),
            CachePolicy::Tracked(noise(reachability_sets_cache_size)),
        )));

        let reachability_relations_store = Arc::new(RwLock::new(DbRelationsStore::with_prefix(
            db.clone(),
            DatabaseStorePrefixes::ReachabilityRelations.as_ref(),
            CachePolicy::Tracked(noise(reachability_relations_cache_size)),
            CachePolicy::Tracked(noise(reachability_relations_children_cache_size)),
        )));

        let max_ghostdag_data_size = size_of::<GhostdagData>()
            + params.mergeset_size_limit as usize * size_of::<Hash>()
            + params.ghostdag_k as usize * size_of::<(Hash, KType)>();

        let ghostdag_stores = Arc::new(
            (0..=params.max_block_level)
                .map(|level| {
                    // = size / 2^level
                    let level_normalized_cache_bytes = ghostdag_cache_bytes.checked_shr(level as u32).unwrap_or(0);
                    let cache_policy = if level_normalized_cache_bytes > 2 * params.pruning_proof_m as usize * max_ghostdag_data_size {
                        CachePolicy::Tracked(noise(level_normalized_cache_bytes))
                    } else {
                        CachePolicy::Unit(noise(2 * params.pruning_proof_m as usize))
                    };
                    let compact_cache_size =
                        max(ghostdag_compact_cache_size.checked_shr(level as u32).unwrap_or(0), 2 * params.pruning_proof_m as usize);
                    Arc::new(DbGhostdagStore::new(db.clone(), level, cache_policy, CachePolicy::Unit(noise(compact_cache_size))))
                })
                .collect_vec(),
        );
        let ghostdag_primary_store = ghostdag_stores[0].clone();
        let daa_excluded_store = Arc::new(DbDaaStore::new(db.clone(), CachePolicy::Unit(noise(daa_excluded_cache_size))));
        let headers_store = Arc::new(DbHeadersStore::new(
            db.clone(),
            CachePolicy::Tracked(noise(headers_cache_bytes)),
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
            Arc::new(DbBlockTransactionsStore::new(db.clone(), CachePolicy::Tracked(noise(transactions_cache_size))));
        let utxo_diffs_store = Arc::new(DbUtxoDiffsStore::new(db.clone(), CachePolicy::Tracked(noise(utxo_diffs_cache_bytes))));
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
