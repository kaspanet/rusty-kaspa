use consensus::{
    model::stores::{
        ghostdag::{DbGhostdagStore, KType},
        reachability::{DbReachabilityStore, StagingReachabilityStore},
        relations::DbRelationsStore,
    },
    pipeline::header_processor::HeaderProcessor,
    processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions},
};
use consensus_core::{block::Block, blockhash, header::Header};
use crossbeam_channel::{unbounded, Receiver, Sender};
use hashes::Hash;
use parking_lot::RwLock;
use rocksdb::WriteBatch;
use std::{sync::Arc, thread};

mod common;

#[test]
fn test_reachability_staging() {
    // Arrange
    let (_tempdir, db) = common::create_temp_db();
    let store = RwLock::new(DbReachabilityStore::new(db.clone(), 10000));
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
    let mut batch = WriteBatch::default();
    {
        let _write_guard = staging.commit(&mut batch).unwrap();
        db.write(batch).unwrap();
    }

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

#[test]
fn test_concurrent_pipeline() {
    let (_tempdir, db) = common::create_temp_db();

    let relations_store = Arc::new(RwLock::new(DbRelationsStore::new(db.clone(), 100000)));
    let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), 100000)));
    let ghostdag_store = Arc::new(DbGhostdagStore::new(db.clone(), 100000));

    let genesis: Hash = 1.into();
    let ghostdag_k: KType = 18;
    let (sender, receiver): (Sender<Arc<Block>>, Receiver<Arc<Block>>) = unbounded();

    let header_processor = Arc::new(HeaderProcessor::new(
        receiver,
        genesis,
        ghostdag_k,
        db,
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
