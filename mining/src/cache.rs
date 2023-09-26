use kaspa_consensus_core::block::BlockTemplate;
use kaspa_core::time::unix_now;
use parking_lot::{Mutex, MutexGuard};
use std::sync::{
    atomic::{AtomicBool, Ordering::SeqCst},
    Arc,
};

/// CACHE_LIFETIME indicates the default duration in milliseconds after which the cached data expires.
const DEFAULT_CACHE_LIFETIME: u64 = 1_000;

pub(crate) struct Inner {
    /// Time, in milliseconds, at which the cache was last updated
    last_update_time: u64,

    /// The optional template
    block_template: Option<Arc<BlockTemplate>>,

    /// Duration in milliseconds after which the cached data expires
    cache_lifetime: u64,
}

impl Inner {
    pub(crate) fn new(cache_lifetime: Option<u64>) -> Self {
        let cache_lifetime = cache_lifetime.unwrap_or(DEFAULT_CACHE_LIFETIME);
        Self { last_update_time: 0, block_template: None, cache_lifetime }
    }

    fn clear(&mut self) {
        self.block_template = None;
    }

    pub(crate) fn get_immutable_cached_template(&self) -> Option<Arc<BlockTemplate>> {
        let now = unix_now();
        // We verify that `now > last update` in order to avoid theoretic clock change bugs
        if now > self.last_update_time + self.cache_lifetime || now < self.last_update_time {
            None
        } else {
            self.block_template.clone()
        }
    }

    pub(crate) fn set_immutable_cached_template(&mut self, block_template: BlockTemplate) -> Arc<BlockTemplate> {
        self.last_update_time = unix_now();
        let block_template = Arc::new(block_template);
        self.block_template = Some(block_template.clone());
        block_template
    }
}

pub(crate) struct BlockTemplateCache {
    inner: Mutex<Inner>,
    clear_flag: AtomicBool,
}

impl BlockTemplateCache {
    pub(crate) fn new(cache_lifetime: Option<u64>) -> Self {
        Self { inner: Mutex::new(Inner::new(cache_lifetime)), clear_flag: AtomicBool::new(false) }
    }

    pub(crate) fn clear(&self) {
        // We avoid blocking on the mutex for clear but rather signal to the next
        // thread acquiring the lock to clear the template
        self.clear_flag.store(true, SeqCst)
    }

    pub(crate) fn lock(&self) -> MutexGuard<Inner> {
        let mut guard = self.inner.lock();
        if self.clear_flag.swap(false, SeqCst) {
            // If clear was signaled, perform the actual clear
            guard.clear();
        }
        guard
    }
}
