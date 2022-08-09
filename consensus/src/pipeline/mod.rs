use crate::model::stores::ghostdag::GhostdagData;
use consensus_core::blockhash::BlockHashes;
use hashes::Hash;
use std::sync::Arc;

pub struct HeaderProcessingContext {
    pub hash: Hash,
    // header: Header,
    // cached_parents: Option<HashArray>,
    // cached_selected_parent: Option<Hash>,
    pub cached_mergeset: Option<BlockHashes>,
    pub staged_ghostdag_data: Option<Arc<GhostdagData>>,
}

impl HeaderProcessingContext {
    pub fn new(hash: Hash) -> Self {
        Self { hash, cached_mergeset: None, staged_ghostdag_data: None }
    }

    pub fn cache_mergeset(&mut self, mergeset: BlockHashes) {
        self.cached_mergeset = Some(mergeset);
    }

    pub fn stage_ghostdag_data(&mut self, ghostdag_data: Arc<GhostdagData>) {
        self.staged_ghostdag_data = Some(ghostdag_data);
    }
}
