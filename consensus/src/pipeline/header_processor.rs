use crate::{
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStore, KType},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStore},
            DB,
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
use rocksdb::WriteBatch;
use std::{ops::DerefMut, sync::Arc};

pub struct HeaderProcessingContext<'a> {
    pub hash: Hash,
    pub header: &'a Header,
    pub cached_mergeset: Option<BlockHashes>,
    pub staged_ghostdag_data: Option<Arc<GhostdagData>>,
}

impl<'a> HeaderProcessingContext<'a> {
    pub fn new(hash: Hash, header: &'a Header) -> Self {
        Self { hash, header, cached_mergeset: None, staged_ghostdag_data: None }
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

    // DB
    db: Arc<DB>,

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
        genesis_hash: Hash, ghostdag_k: KType, db: Arc<DB>, relations_store: Arc<RwLock<DbRelationsStore>>,
        reachability_store: Arc<RwLock<DbReachabilityStore>>, ghostdag_store: Arc<DbGhostdagStore>,
    ) -> Self {
        Self {
            receiver,
            // sender,
            genesis_hash,
            // ghostdag_k,
            db,
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
        // Create processing context
        let mut ctx = HeaderProcessingContext::new(header.hash, header);

        // Run GHOSTDAG for the new header
        self.ghostdag_manager
            .add_block(&mut ctx, header.hash);

        let data = ctx.staged_ghostdag_data.unwrap();

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

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        // Write block relations
        let mut relations_write = self.relations_store.write(); // Note we lock until the batch is written
        relations_write
            .insert_batch(&mut batch, header.hash, BlockHashes::new(header.parents.clone()))
            .unwrap();

        // Write GHOSTDAG data
        // Note: GHOSTDAG data is append-only and requires no lock
        self.ghostdag_store
            .insert_batch(&mut batch, ctx.hash, data)
            .unwrap();

        // Write reachability data
        let write_guard = staging.commit(&mut batch).unwrap(); // Note we hold the lock until the batch is written

        // Flush the batch to the DB
        self.db.write(batch).unwrap();
    }

    pub fn insert_genesis_if_needed(self: &Arc<HeaderProcessor>) {
        let header = Header::new(self.genesis_hash, vec![]);
        let mut ctx = HeaderProcessingContext::new(self.genesis_hash, &header);
        self.ghostdag_manager.init(&mut ctx);

        // TODO: must use batch writing as well
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
