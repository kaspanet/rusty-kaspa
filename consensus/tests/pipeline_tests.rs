use consensus::{
    consensus::Consensus,
    model::stores::{
        ghostdag::KType,
        reachability::{DbReachabilityStore, StagingReachabilityStore},
    },
    processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions},
};
use consensus_core::{block::Block, blockhash};
use hashes::Hash;
use parking_lot::RwLock;
use rand_distr::{Distribution, Poisson};
use rocksdb::WriteBatch;
use std::sync::Arc;

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
    let genesis: Hash = 1.into();
    let ghostdag_k: KType = 18;

    let (_tempdir, db) = common::create_temp_db();
    let consensus = Consensus::new(db, genesis, ghostdag_k);
    let wait_handle = consensus.init();

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
        // Submit to consensus twice to make sure duplicates are handled
        let b = Arc::new(block);
        consensus.validate_and_insert_block(Arc::clone(&b));
        consensus.validate_and_insert_block(b);
    }

    let (store, _) = consensus.drop();
    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = store.read().clone_with_new_cache(10000);

    wait_handle.join().unwrap();

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
fn test_pipeline_stream() {
    let genesis: Hash = blockhash::new_unique();
    let ghostdag_k: KType = 18;
    let bps = 8;
    let delay = 2;

    let poi = Poisson::new((bps * delay) as f64).unwrap();
    let mut thread_rng = rand::thread_rng();

    let (_tempdir, db) = common::create_temp_db();
    let consensus = Consensus::new(db, genesis, ghostdag_k);
    let wait_handle = consensus.init();

    let mut tips = vec![genesis];
    let mut total = 10000i64;
    while total > 0 {
        let v = poi.sample(&mut thread_rng) as i64;
        if v == 0 {
            continue;
        }
        total -= v;
        // println!("{} is from a Poisson(2) distribution", v);
        let mut new_tips = Vec::with_capacity(v as usize);
        for _ in 0..v {
            let hash = blockhash::new_unique();
            new_tips.push(hash);
            let b = Block::new(hash, tips.clone());
            // Submit to consensus
            consensus.validate_and_insert_block(Arc::new(b));
        }
        tips = new_tips;
    }

    let (_store, _) = consensus.drop();
    // Clone with a new cache in order to verify correct writes to the DB itself
    // let store = store.read().clone_with_new_cache(10000);

    wait_handle.join().unwrap();

    // Assert intervals
    // store
    //     .validate_intervals(blockhash::ORIGIN)
    //     .unwrap();
}
