use crate::{
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader, KType},
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::DbRelationsStore,
            DB,
        },
    },
    processes::{ghostdag::protocol::GhostdagManager, reachability::inquirer},
};
use consensus_core::{block::Block, blockhash::BlockHashes, header::Header};
use crossbeam::select;
use crossbeam_channel::{Receiver, Sender};
use hashes::Hash;
use parking_lot::{Mutex, RwLock};
use rocksdb::WriteBatch;
use std::{
    collections::{hash_map::Entry::Vacant, HashMap, HashSet},
    sync::Arc,
};

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

pub enum BlockTask {
    Yield,
    Exit,
    External(Arc<Block>),
    Resent(Arc<Block>),
}

struct PendingBlocksManager {
    /// Holds pending block hashes and their dependent blocks
    pub pending: HashMap<Hash, Vec<Arc<Block>>>,

    /// Holds the currently processed set of blocks
    pub processing: HashSet<Hash>,
}

impl PendingBlocksManager {
    fn new() -> Self {
        Self { pending: HashMap::new(), processing: HashSet::new() }
    }
}

pub struct HeaderProcessor {
    // Channels
    receiver: Receiver<BlockTask>,
    sender: Sender<BlockTask>, // Used for self-sending pending blocks

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

    // Pending blocks management
    pending_manager: Mutex<PendingBlocksManager>,
}

impl HeaderProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>, sender: Sender<BlockTask>, genesis_hash: Hash, ghostdag_k: KType, db: Arc<DB>,
        relations_store: Arc<RwLock<DbRelationsStore>>, reachability_store: Arc<RwLock<DbReachabilityStore>>,
        ghostdag_store: Arc<DbGhostdagStore>,
    ) -> Self {
        Self {
            receiver,
            sender,
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
            pending_manager: Mutex::new(PendingBlocksManager::new()),
        }
    }

    pub fn worker(self: &Arc<HeaderProcessor>) {
        let receiver = self.receiver.clone();
        // let sender = self.sender.clone();

        let mut exiting = false;
        loop {
            if exiting {
                let manager = self.pending_manager.lock();
                if manager.pending.is_empty() {
                    break;
                }
            }
            select! {
                recv(receiver) -> data => {
                    if let Ok(task) = data {
                        match task {
                            BlockTask::Yield => (),
                            BlockTask::Exit => exiting = true,
                            BlockTask::External(block) | BlockTask::Resent(block) => self.queue_block(block),
                        };
                    } else {
                        // All senders are dropped, exit
                        break;
                    }
                }
            }
        }
    }

    fn queue_block(self: &Arc<HeaderProcessor>, block: Arc<Block>) {
        let hash = block.header.hash;

        {
            // Lock pending manager. The contention around the manager is
            // expected to be negligible in header processing time
            let mut manager = self.pending_manager.lock();

            if let Vacant(e) = manager.pending.entry(hash) {
                e.insert(Vec::new());
            }

            for parent in block.header.parents.iter() {
                if let Some(deps) = manager.pending.get_mut(parent) {
                    deps.push(block);
                    return; // The block will be resent once the pending parent completes processing
                }
            }

            if !manager.processing.insert(hash) {
                return; // Block is already being processed
            }
        }

        let processor = self.clone();
        rayon::spawn(move || {
            // TODO: report duplicate block to job sender
            if processor.header_was_processed(hash) {
                return;
            }

            // TODO: report missing parents to job sender (currently will panic for missing keys)

            processor.process_header(&block.header);
            let mut manager = processor.pending_manager.lock();

            assert!(manager.processing.remove(&hash), "processed block is expected to be in processing set");

            let deps = manager
                .pending
                .remove(&hash)
                .expect("processed block is expected to be in pending map");
            for dep in deps {
                // Resend the block through the channel.
                processor
                    .sender
                    .send(BlockTask::Resent(dep))
                    .unwrap();
            }
            // Yield the receiver to check its exit state
            processor.sender.send(BlockTask::Yield).unwrap();
        });
    }

    fn header_was_processed(self: &Arc<HeaderProcessor>, hash: Hash) -> bool {
        // For now, use `reachability_store.has` as an indication for processing.
        // TODO: block status store should be used.
        self.reachability_store.read().has(hash).unwrap()
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
