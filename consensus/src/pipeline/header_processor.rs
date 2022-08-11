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
    processes::{ghostdag::protocol::GhostdagManager, reachability::inquirer as reachability},
};
use consensus_core::{block::Block, blockhash::BlockHashes, header::Header};
use crossbeam::select;
use crossbeam_channel::{unbounded, Receiver, Sender};
use hashes::Hash;
use parking_lot::{Condvar, Mutex, RwLock};
use rocksdb::WriteBatch;
use std::{
    collections::{hash_map::Entry::Vacant, HashMap, HashSet},
    sync::{atomic::Ordering, Arc},
};

use super::ProcessingCounters;

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
    Exit,
    Process(Arc<Block>),
}

pub enum PendingTask {
    Yield,
    Process(Arc<Block>),
}

const SIGNAL_THRESHOLD: usize = 18;

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

    // Channels used for resending pending blocks
    _pending_resend: Sender<PendingTask>,
    _pending_receiver: Receiver<PendingTask>,

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

    // Counters
    counters: Arc<ProcessingCounters>,

    /// Used to signal workers availability
    pub signal: Condvar,
}

impl HeaderProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>, genesis_hash: Hash, ghostdag_k: KType, db: Arc<DB>,
        relations_store: Arc<RwLock<DbRelationsStore>>, reachability_store: Arc<RwLock<DbReachabilityStore>>,
        ghostdag_store: Arc<DbGhostdagStore>, counters: Arc<ProcessingCounters>,
    ) -> Self {
        let (_pending_resend, _pending_receiver): (Sender<PendingTask>, Receiver<PendingTask>) = unbounded();

        Self {
            receiver,
            _pending_resend,
            _pending_receiver,
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
            counters,
            signal: Condvar::new(),
        }
    }

    pub fn worker(self: &Arc<HeaderProcessor>) {
        loop {
            select! {
                recv(self.receiver) -> data => {
                    if let Ok(task) = data {
                        match task {
                            BlockTask::Exit => break,
                            BlockTask::Process(block) => {

                                let mut manager = self.pending_manager.lock();

                                if let Vacant(e) = manager.pending.entry(block.header.hash) {
                                    e.insert(Vec::new());
                                    if manager.pending.len() > SIGNAL_THRESHOLD {
                                        self.signal.wait(&mut manager);
                                    }

                                    let processor = self.clone();
                                    rayon::spawn(move || {
                                        processor.queue_block(block);
                                    });
                                }
                            }
                        };
                    } else {
                        // All senders are dropped
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

        // TODO: report duplicate block to job sender
        if self.header_was_processed(hash) {
            return;
        }

        // TODO: report missing parents to job sender (currently will panic for missing keys)

        self.process_header(&block.header);

        let mut manager = self.pending_manager.lock();
        assert!(manager.processing.remove(&hash), "processed block is expected to be in processing set");
        let deps = manager
            .pending
            .remove(&hash)
            .expect("processed block is expected to be in pending map");

        if manager.pending.len() == SIGNAL_THRESHOLD {
            self.signal.notify_one();
        }

        for dep in deps {
            let processor = self.clone();
            rayon::spawn(move || processor.queue_block(dep));
        }
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

        // Report to counters
        self.counters
            .header_counts
            .fetch_add(1, Ordering::SeqCst);
        self.counters
            .dep_counts
            .fetch_add(header.parents.len() as u64, Ordering::SeqCst);
    }

    fn commit_header(self: &Arc<HeaderProcessor>, ctx: HeaderProcessingContext, header: &Header) {
        let ghostdag_data = ctx.ghostdag_data.unwrap();

        // Create staging reachability store. We use an upgradable read here to avoid concurrent
        // staging reachability operations. PERF: we assume that reachability processing time << header processing
        // time, and thus serializing this part will do no harm. However this should be benchmarked. The
        // alternative is to create a separate ReachabilityProcessor and to manage things more tightly.
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());

        // Add block to staging reachability
        reachability::add_block(
            &mut staging,
            ctx.hash,
            ghostdag_data.selected_parent,
            &mut ctx.mergeset.unwrap().iter().cloned(),
        )
        .unwrap();
        // Hint a new tip.
        // TODO: imp header tips store and call this only for an actual header selected tip
        reachability::hint_virtual_selected_parent(&mut staging, ctx.hash).unwrap();

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
