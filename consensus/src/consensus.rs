use crate::{
    model::stores::{
        ghostdag::{DbGhostdagStore, KType},
        reachability::DbReachabilityStore,
        relations::DbRelationsStore,
        DB,
    },
    pipeline::header_processor::HeaderProcessor,
};
use consensus_core::block::Block;
use crossbeam_channel::{unbounded, Receiver, Sender};
use hashes::Hash;
use parking_lot::RwLock;
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};

pub struct Consensus {
    db: Arc<DB>,
    block_sender: Sender<Arc<Block>>,
    header_processor: Arc<HeaderProcessor>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>, // TEMP
}

impl Consensus {
    pub fn new(db: Arc<DB>, genesis: Hash, ghostdag_k: KType) -> Self {
        let relations_store = Arc::new(RwLock::new(DbRelationsStore::new(db.clone(), 100000)));
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), 100000)));
        let ghostdag_store = Arc::new(DbGhostdagStore::new(db.clone(), 100000));

        let (sender, receiver): (Sender<Arc<Block>>, Receiver<Arc<Block>>) = unbounded();

        let header_processor = Arc::new(HeaderProcessor::new(
            receiver,
            genesis,
            ghostdag_k,
            db.clone(),
            relations_store,
            reachability_store.clone(),
            ghostdag_store,
        ));

        Self { db, block_sender: sender, header_processor, reachability_store }
    }

    pub fn init(&self) -> JoinHandle<()> {
        self.header_processor.insert_genesis_if_needed();
        let header_processor = self.header_processor.clone();
        // Spawn an asynchronous header processor.
        thread::spawn(move || header_processor.worker())
    }

    pub fn validate_and_insert_block(&self, block: Arc<Block>) {
        self.block_sender.send(block).unwrap();
    }

    /// TEMP
    pub fn drop(self) -> Arc<RwLock<DbReachabilityStore>> {
        self.reachability_store
    }
}
