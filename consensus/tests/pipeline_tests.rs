use consensus::{
    consensus::Consensus,
    model::stores::reachability::{DbReachabilityStore, StagingReachabilityStore},
    params::MAINNET_PARAMS,
    processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions},
    test_helpers::block_from_precomputed_hash,
};
use consensus_core::blockhash;
use hashes::Hash;
use parking_lot::RwLock;
use rand_distr::{Distribution, Poisson};
use rocksdb::WriteBatch;
use std::{cmp::min, sync::Arc};

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

    let mut params = MAINNET_PARAMS;
    params.genesis_hash = 1.into();

    let consensus = Consensus::new(db, &params);
    let wait_handle = consensus.init();

    let blocks = vec![
        block_from_precomputed_hash(2.into(), vec![1.into()]),
        block_from_precomputed_hash(3.into(), vec![1.into()]),
        block_from_precomputed_hash(4.into(), vec![2.into(), 3.into()]),
        block_from_precomputed_hash(5.into(), vec![4.into()]),
        block_from_precomputed_hash(6.into(), vec![1.into()]),
        block_from_precomputed_hash(7.into(), vec![5.into(), 6.into()]),
        block_from_precomputed_hash(8.into(), vec![1.into()]),
        block_from_precomputed_hash(9.into(), vec![1.into()]),
        block_from_precomputed_hash(10.into(), vec![7.into(), 8.into(), 9.into()]),
        block_from_precomputed_hash(11.into(), vec![1.into()]),
        block_from_precomputed_hash(12.into(), vec![11.into(), 10.into()]),
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
fn test_concurrent_pipeline_random() {
    let genesis: Hash = blockhash::new_unique();
    let bps = 8;
    let delay = 2;

    let poi = Poisson::new((bps * delay) as f64).unwrap();
    let mut thread_rng = rand::thread_rng();

    let (_tempdir, db) = common::create_temp_db();

    let mut params = MAINNET_PARAMS;
    params.genesis_hash = genesis;

    let consensus = Consensus::new(db, &params);
    let wait_handle = consensus.init();

    let mut tips = vec![genesis];
    let mut total = 1000i64;
    while total > 0 {
        let mut v = poi.sample(&mut thread_rng) as i64;
        v = min(params.max_block_parents as i64, v);
        if v == 0 {
            continue;
        }
        total -= v;
        // println!("{} is from a Poisson(2) distribution", v);
        let mut new_tips = Vec::with_capacity(v as usize);
        for _ in 0..v {
            let hash = blockhash::new_unique();
            new_tips.push(hash);
            let b = block_from_precomputed_hash(hash, tips.clone());
            // Submit to consensus
            consensus.validate_and_insert_block(Arc::new(b));
        }
        tips = new_tips;
    }

    let (store, _) = consensus.drop();
    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = store.read().clone_with_new_cache(10000);

    wait_handle.join().unwrap();

    // Assert intervals
    store
        .validate_intervals(blockhash::ORIGIN)
        .unwrap();
}
