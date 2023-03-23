use kaspa_consensus_core::block::BlockTemplate;
use kaspa_core::time::unix_now;
use std::sync::Arc;

/// CACHE_LIFETIME indicates the default duration in milliseconds after which the cached data expires.
const DEFAULT_CACHE_LIFETIME: u64 = 1_000;

pub(crate) struct BlockTemplateCache {
    /// Time, in milliseconds, when the cache was last updated
    last_update_time: u64,
    block_template: Option<Arc<BlockTemplate>>,

    /// Duration in milliseconds after which the cached data expires
    cache_lifetime: u64,
}

impl BlockTemplateCache {
    pub(crate) fn new(cache_lifetime: Option<u64>) -> Self {
        let cache_lifetime = cache_lifetime.unwrap_or(DEFAULT_CACHE_LIFETIME);
        Self { last_update_time: 0, block_template: None, cache_lifetime }
    }

    pub(crate) fn clear(&mut self) {
        // The cache timer is reset to 0 so its lifetime is expired.
        self.last_update_time = 0;
        self.block_template = None;
    }

    pub(crate) fn get_immutable_cached_template(&self) -> Option<Arc<BlockTemplate>> {
        let now = unix_now();
        // We verify that `now > last update` in order to avoid theoretic clock change bugs
        if now < self.last_update_time || now - self.last_update_time > self.cache_lifetime {
            None
        } else {
            Some(self.block_template.as_ref().unwrap().clone())
        }
    }

    pub(crate) fn set_immutable_cached_template(&mut self, block_template: BlockTemplate) -> Arc<BlockTemplate> {
        self.last_update_time = unix_now();
        self.block_template = Some(Arc::new(block_template));
        self.block_template.as_ref().unwrap().clone()
    }
}
