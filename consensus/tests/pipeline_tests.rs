use std::{ops::DerefMut, sync::Arc, thread};

use consensus::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore, KType},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStore},
        },
    },
    pipeline::HeaderProcessingContext,
    processes::{
        ghostdag::protocol::GhostdagManager,
        reachability::{
            inquirer,
            tests::{DagBlock, DagBuilder, StoreValidationExtensions},
        },
    },
};
use consensus_core::blockhash::{self, BlockHashes};
use crossbeam::select;
use crossbeam_channel::{unbounded, Receiver, Sender};
use hashes::Hash;
use parking_lot::RwLock;

mod common;

#[test]
fn test_reachability_staging() {
    // Arrange
    let (_tempdir, db) = common::create_temp_db();
    let store = RwLock::new(DbReachabilityStore::new(db, 10000));
    let mut staging = StagingReachabilityStore::new(store.upgradable_read());

    // Act
    DagBuilder::new(&mut staging)
        .init()
        .add_block(DagBlock::new(1.into(), vec![blockhash::ORIGIN]))
        .add_block(DagBlock::new(2.into(), vec![1.into()]))
        .add_block(DagBlock::new(3.into(), vec![1.into()]))
        .add_block(DagBlock::new(4.into(), vec![2.into(), 3.into()]))
        .add_block(DagBlock::new(5.into(), vec![4.into()]))
        .add_block(DagBlock::new(6.into(), vec![1.into()]))
        .add_block(DagBlock::new(7.into(), vec![5.into(), 6.into()]))
        .add_block(DagBlock::new(8.into(), vec![1.into()]))
        .add_block(DagBlock::new(9.into(), vec![1.into()]))
        .add_block(DagBlock::new(10.into(), vec![7.into(), 8.into(), 9.into()]))
        .add_block(DagBlock::new(11.into(), vec![1.into()]))
        .add_block(DagBlock::new(12.into(), vec![11.into(), 10.into()]));

    // Commit the staging changes
    staging.commit().unwrap();

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = store.read().clone_with_new_cache(10000);

    // Assert intervals
    store
        .validate_intervals(blockhash::ORIGIN)
        .unwrap();

    // Assert genesis
    for i in 2u64..=12 {
        assert!(store.in_past_of(1, i));
    }

    // Assert some futures
    assert!(store.in_past_of(2, 4));
    assert!(store.in_past_of(2, 5));
    assert!(store.in_past_of(2, 7));
    assert!(store.in_past_of(5, 10));
    assert!(store.in_past_of(6, 10));
    assert!(store.in_past_of(10, 12));
    assert!(store.in_past_of(11, 12));

    // Assert some anticones
    assert!(store.are_anticone(2, 3));
    assert!(store.are_anticone(2, 6));
    assert!(store.are_anticone(3, 6));
    assert!(store.are_anticone(5, 6));
    assert!(store.are_anticone(3, 8));
    assert!(store.are_anticone(11, 2));
    assert!(store.are_anticone(11, 4));
    assert!(store.are_anticone(11, 6));
    assert!(store.are_anticone(11, 9));
}

pub struct Header {
    pub hash: Hash, // TEMP: until consensushashing is ready
    pub parents: Vec<Hash>,
}

impl Header {
    pub fn new(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { hash, parents }
    }
}

pub struct Block {
    pub header: Header,
}

impl Block {
    pub fn new(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { header: Header::new(hash, parents) }
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
    relations_store: Arc<DbRelationsStore>, // TODO: needs a lock as it is not append-only
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    ghostdag_store: Arc<DbGhostdagStore>,

    // Managers and services
    ghostdag_manager: GhostdagManager<DbGhostdagStore, DbRelationsStore, MTReachabilityService<DbReachabilityStore>>,
}

impl HeaderProcessor {
    pub fn new(
        receiver: Receiver<Arc<Block>>, /*, sender: Sender<Arc<Block>>*/
        genesis_hash: Hash, ghostdag_k: KType, relations_store: Arc<DbRelationsStore>,
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
                relations_store,
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
            .set_parents(header.hash, BlockHashes::new(header.parents.clone()))
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

#[test]
fn test_concurrent_pipeline() {
    let (_tempdir, db) = common::create_temp_db();

    let relations_store = Arc::new(DbRelationsStore::new(db.clone(), 100000));
    let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), 100000)));
    let ghostdag_store = Arc::new(DbGhostdagStore::new(db, 100000));

    let genesis: Hash = 1.into();
    let ghostdag_k: KType = 18;
    let (sender, receiver): (Sender<Arc<Block>>, Receiver<Arc<Block>>) = unbounded();

    let header_processor = Arc::new(HeaderProcessor::new(
        receiver,
        genesis,
        ghostdag_k,
        relations_store,
        reachability_store.clone(),
        ghostdag_store,
    ));
    header_processor.insert_genesis_if_needed(&Header::new(genesis, vec![]));

    // Spawn an asynchronous header processor.
    let handle = thread::spawn(move || header_processor.worker());

    let blocks = vec![
        Block::new(2.into(), vec![1.into()]),
        Block::new(3.into(), vec![1.into()]),
        Block::new(4.into(), vec![2.into(), 3.into()]),
        Block::new(5.into(), vec![4.into()]),
        Block::new(6.into(), vec![1.into()]),
        Block::new(7.into(), vec![5.into(), 6.into()]),
        Block::new(8.into(), vec![1.into()]),
        Block::new(9.into(), vec![1.into()]),
        Block::new(10.into(), vec![7.into(), 8.into(), 9.into()]),
        Block::new(11.into(), vec![1.into()]),
        Block::new(12.into(), vec![11.into(), 10.into()]),
    ];

    for block in blocks {
        // Send to header processor
        sender.send(Arc::new(block)).unwrap();
    }
    drop(sender);
    handle.join().unwrap();

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = reachability_store
        .read()
        .clone_with_new_cache(10000);

    // Assert intervals
    store
        .validate_intervals(blockhash::ORIGIN)
        .unwrap();

    // Assert genesis
    for i in 2u64..=12 {
        assert!(store.in_past_of(1, i));
    }

    // Assert some futures
    assert!(store.in_past_of(2, 4));
    assert!(store.in_past_of(2, 5));
    assert!(store.in_past_of(2, 7));
    assert!(store.in_past_of(5, 10));
    assert!(store.in_past_of(6, 10));
    assert!(store.in_past_of(10, 12));
    assert!(store.in_past_of(11, 12));

    // Assert some anticones
    assert!(store.are_anticone(2, 3));
    assert!(store.are_anticone(2, 6));
    assert!(store.are_anticone(3, 6));
    assert!(store.are_anticone(5, 6));
    assert!(store.are_anticone(3, 8));
    assert!(store.are_anticone(11, 2));
    assert!(store.are_anticone(11, 4));
    assert!(store.are_anticone(11, 6));
    assert!(store.are_anticone(11, 9));
}
