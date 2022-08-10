use crate::{
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader, KType},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::DbRelationsStore,
            DB,
        },
    },
    processes::{ghostdag::protocol::GhostdagManager, reachability::inquirer},
};
use consensus_core::{block::Block, blockhash::BlockHashes, header::Header};
use crossbeam::select;
use crossbeam_channel::Receiver;
use hashes::Hash;
use parking_lot::RwLock;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub struct HeaderProcessingContext<'a> {
    pub hash: Hash,
    pub header: &'a Header,

    /// Mergeset w/o selected parent
    pub mergeset: Option<BlockHashes>,

    // Staging data
    pub ghostdag_data: Option<Arc<GhostdagData>>,
}

impl<'a> HeaderProcessingContext<'a> {
    pub fn new(hash: Hash, header: &'a Header) -> Self {
        Self { hash, header, mergeset: None, ghostdag_data: None }
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

        //
        // TODO: imp all remaining header validation and processing steps :)
        //

        self.commit_header(ctx, header);
    }

    fn commit_header(self: &Arc<HeaderProcessor>, ctx: HeaderProcessingContext, header: &Header) {
        let ghostdag_data = ctx.ghostdag_data.unwrap();

        // Create staging reachability store. We use an upgradable read here to avoid concurrent
        // staging reachability operations. PERF: we assume that reachability processing time << header processing
        // time, and thus serializing this part will do no harm. However this should be benchmarked. The
        // alternative is to create a separate ReachabilityProcessor and to manage things more tightly.
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());
        // Add block to staging reachability
        inquirer::add_block(
            &mut staging,
            ctx.hash,
            ghostdag_data.selected_parent,
            &mut ctx.mergeset.unwrap().iter().cloned(),
        )
        .unwrap();

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        // Write GHOSTDAG data. GHOSTDAG data is append-only and requires no lock
        self.ghostdag_store
            .insert_batch(&mut batch, ctx.hash, ghostdag_data)
            .unwrap();

        // Write block relations. Block relations are not append-only, since children arrays of parents are
        // updated as well, hence the need to write lock.
        let mut relations_write = self.relations_store.write(); // Note we lock until the batch is written
        relations_write
            .insert_batch(&mut batch, header.hash, BlockHashes::new(header.parents.clone()))
            .unwrap();

        // Write reachability data. Only at this brief moment the reachability store is locked for reads.
        // We take special care for this since reachability read queries are used throughout the system frequently.
        let write_guard = staging.commit(&mut batch).unwrap(); // Note we hold the lock until the batch is written

        // Flush the batch to the DB
        self.db.write(batch).unwrap();
    }

    pub fn process_genesis_if_needed(self: &Arc<HeaderProcessor>) {
        if self
            .ghostdag_store
            .has(self.genesis_hash, false)
            .unwrap()
        {
            return;
        }
        let header = Header::new(self.genesis_hash, vec![]);
        let mut ctx = HeaderProcessingContext::new(self.genesis_hash, &header);
        self.ghostdag_manager
            .add_genesis_if_needed(&mut ctx);
        self.commit_header(ctx, &header);
    }
}
