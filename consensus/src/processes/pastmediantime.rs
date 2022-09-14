use std::sync::Arc;

use crate::model::stores::{
    block_window_cache::{BlockWindowCacheReader, BlockWindowHeap},
    ghostdag::{GhostdagData, GhostdagStoreReader},
    headers::HeaderStoreReader,
};

use super::dagtraversalmanager::DagTraversalManager;

#[derive(Clone)]
pub struct PastMedianTimeManager<T: HeaderStoreReader, U: GhostdagStoreReader, V: BlockWindowCacheReader> {
    headers_store: Arc<T>,
    dag_traversal_manager: DagTraversalManager<U, V>,
    timestamp_deviation_tolerance: usize,
    genesis_timestamp: u64,
}

impl<T: HeaderStoreReader, U: GhostdagStoreReader, V: BlockWindowCacheReader> PastMedianTimeManager<T, U, V> {
    pub fn new(
        headers_store: Arc<T>,
        dag_traversal_manager: DagTraversalManager<U, V>,
        timestamp_deviation_tolerance: usize,
        genesis_timestamp: u64,
    ) -> Self {
        Self { headers_store, dag_traversal_manager, timestamp_deviation_tolerance, genesis_timestamp }
    }

    pub fn calc_past_median_time(&self, ghostdag_data: Arc<GhostdagData>) -> (u64, BlockWindowHeap) {
        let window = self.dag_traversal_manager.block_window(ghostdag_data, 2 * self.timestamp_deviation_tolerance - 1);

        if window.is_empty() {
            return (self.genesis_timestamp, Default::default());
        }

        let mut window_timestamps: Vec<u64> =
            window.iter().map(|item| self.headers_store.get_timestamp(item.0.hash).unwrap()).collect();
        window_timestamps.sort_unstable(); // This is deterministic because we sort u64
        (window_timestamps[window_timestamps.len() / 2], window)
    }
}
