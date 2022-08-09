use crate::{
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStore, KType},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStore},
        },
    },
    processes::{ghostdag::protocol::GhostdagManager, reachability::inquirer},
};
use consensus_core::{
    block::Block,
    blockhash::{self, BlockHashes},
    header::Header,
};
use crossbeam::select;
use crossbeam_channel::Receiver;
use hashes::Hash;
use parking_lot::RwLock;
use std::{ops::DerefMut, sync::Arc};

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

pub struct HeaderProcessor {
    // Channels
    receiver: Receiver<Arc<Block>>,
    // sender: Sender<Arc<Block>>,

    // Config
    genesis_hash: Hash,
    // ghostdag_k: KType,

    // Stores
    relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    ghostdag_store: Arc<DbGhostdagStore>,

    // Managers and services
    ghostdag_manager: GhostdagManager<
        DbGhostdagStore,
        MTRelationsService<DbRelationsStore>,
        MTReachabilityService<DbReachabilityStore>,
    >,
}

impl HeaderProcessor {
    pub fn new(
        receiver: Receiver<Arc<Block>>, /*, sender: Sender<Arc<Block>>*/
        genesis_hash: Hash, ghostdag_k: KType, relations_store: Arc<RwLock<DbRelationsStore>>,
        reachability_store: Arc<RwLock<DbReachabilityStore>>, ghostdag_store: Arc<DbGhostdagStore>,
    ) -> Self {
        Self {
            receiver,
            // sender,
            genesis_hash,
            // ghostdag_k,
            relations_store: relations_store.clone(),
            reachability_store: reachability_store.clone(),
            ghostdag_store: ghostdag_store.clone(),
            ghostdag_manager: GhostdagManager::new(
                genesis_hash,
                ghostdag_k,
                ghostdag_store,
                Arc::new(MTRelationsService::new(relations_store)),
                Arc::new(MTReachabilityService::new(reachability_store)),
            ),
        }
    }

    pub fn worker(self: &Arc<HeaderProcessor>) {
        let receiver = self.receiver.clone();
        // let sender = self.sender.clone();
        loop {
            select! {
                recv(receiver) -> data => {
                    if let Ok(block) = data {
                        // TODO: spawn the task to a thread-pool and manage dependencies
                        self.process_header(&block.header);
                        // sender.send(block).unwrap();
                    } else {
                        // All senders are dropped, break
                        break;
                    }
                }
            }
        }
    }

    fn process_header(self: &Arc<HeaderProcessor>, header: &Header) {
        // Write parents (TODO: should be a staged and batched)
        self.relations_store
            .write()
            .insert(header.hash, BlockHashes::new(header.parents.clone()))
            .unwrap();

        // Create processing context
        let mut ctx = HeaderProcessingContext::new(header.hash);

        // Add the block to GHOSTDAG
        self.ghostdag_manager
            .add_block(&mut ctx, header.hash);

        // Commit staged GHOSTDAG data (TODO: batch)
        let data = ctx.staged_ghostdag_data.unwrap();
        self.ghostdag_store
            .insert(ctx.hash, data.clone())
            .unwrap();

        // Create staging reachability store
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());
        // Add block to staging reachability
        inquirer::add_block(
            &mut staging,
            ctx.hash,
            data.selected_parent,
            &mut ctx.cached_mergeset.unwrap().iter().cloned(),
        )
        .unwrap();
        // Commit the staging changes
        staging.commit().unwrap();
    }

    pub fn insert_genesis_if_needed(self: &Arc<HeaderProcessor>, header: &Header) {
        assert_eq!(header.hash, self.genesis_hash);
        assert_eq!(header.parents.len(), 0);

        let mut ctx = HeaderProcessingContext::new(self.genesis_hash);
        self.ghostdag_manager.init(&mut ctx);

        if let Some(data) = ctx.staged_ghostdag_data {
            self.relations_store
                .write()
                .insert(self.genesis_hash, BlockHashes::new(Vec::new()))
                .unwrap();
            self.ghostdag_store
                .insert(ctx.hash, data)
                .unwrap();
            let mut write_guard = self.reachability_store.write();
            inquirer::init(write_guard.deref_mut()).unwrap();
            inquirer::add_block(write_guard.deref_mut(), self.genesis_hash, blockhash::ORIGIN, &mut std::iter::empty())
                .unwrap();
        }
    }
}
