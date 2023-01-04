use consensus_core::block::BlockTemplate;
use std::{
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

/// CACHE_LIFETIME indicates the duration in milliseconds after which the cached data expires.
const CACHE_LIFETIME: u64 = 1000;

pub(crate) struct BlockTemplateCache {
    /// Time, in milliseconds, when the cache was last updated
    last_update_time: u64,
    block_template: Option<Rc<BlockTemplate>>,
}

impl BlockTemplateCache {
    pub(crate) fn new() -> Self {
        Self { last_update_time: 0, block_template: None }
    }

    pub(crate) fn clear(&mut self) {
        // This differs from golang implementation.
        // The cache timer is reset to 0 so its lifetime is expired.
        self.last_update_time = 0;
        self.block_template = None;
    }

    pub(crate) fn get_immutable_cached_template(&self) -> Option<Rc<BlockTemplate>> {
        if SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 - self.last_update_time > CACHE_LIFETIME {
            None
        } else {
            Some(self.block_template.as_ref().unwrap().clone())
        }
    }

    pub(crate) fn set_immutable_cached_template(&mut self, block_template: BlockTemplate) -> Rc<BlockTemplate> {
        self.last_update_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        self.block_template = Some(Rc::new(block_template));
        self.block_template.as_ref().unwrap().clone()
    }
}
