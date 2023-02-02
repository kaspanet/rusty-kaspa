use consensus::{
    config::ConfigBuilder,
    consensus::test_consensus::{create_temp_db, TestConsensus},
    model::stores::reachability::{DbReachabilityStore, StagingReachabilityStore},
    params::MAINNET_PARAMS,
    processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions},
};
use consensus_core::{blockhash, blockstatus::BlockStatus, errors::block::RuleError};
use futures_util::future::join_all;
use hashes::Hash;
use parking_lot::RwLock;
use rand_distr::{Distribution, Poisson};
use rocksdb::WriteBatch;
use std::cmp::min;
use tokio::join;

mod common;

#[test]
fn test_reachability_staging() {
    // Arrange
    let (_temp_db_lifetime, db) = create_temp_db();
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
    store.validate_intervals(blockhash::ORIGIN).unwrap();

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

#[tokio::test]
async fn test_concurrent_pipeline() {
    let (_temp_db_lifetime, db) = create_temp_db();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.genesis_hash = 1.into()).build();
    let consensus = TestConsensus::new(db, &config);
    let wait_handles = consensus.init();

    let blocks = vec![
        (2.into(), vec![1.into()]),
        (3.into(), vec![1.into()]),
        (4.into(), vec![2.into(), 3.into()]),
        (5.into(), vec![4.into()]),
        (6.into(), vec![1.into()]),
        (7.into(), vec![5.into(), 6.into()]),
        (8.into(), vec![1.into()]),
        (9.into(), vec![1.into()]),
        (10.into(), vec![7.into(), 8.into(), 9.into()]),
        (11.into(), vec![1.into()]),
        (12.into(), vec![11.into(), 10.into()]),
    ];

    for (hash, parents) in blocks {
        // Submit to consensus twice to make sure duplicates are handled
        let b = consensus.build_block_with_parents(hash, parents).to_immutable();
        let results = join!(consensus.validate_and_insert_block(b.clone()), consensus.validate_and_insert_block(b));
        results.0.unwrap();
        results.1.unwrap();
    }

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = consensus.reachability_store().read().clone_with_new_cache(10000);

    // Assert intervals
    store.validate_intervals(blockhash::ORIGIN).unwrap();

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

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn test_concurrent_pipeline_random() {
    let genesis: Hash = blockhash::new_unique();
    let bps = 8;
    let delay = 2;

    let poi = Poisson::new((bps * delay) as f64).unwrap();
    let mut thread_rng = rand::thread_rng();

    let (_temp_db_lifetime, db) = create_temp_db();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.genesis_hash = genesis).build();
    let consensus = TestConsensus::new(db, &config);
    let wait_handles = consensus.init();

    let mut tips = vec![genesis];
    let mut total = 1000i64;
    while total > 0 {
        let v = min(config.max_block_parents as i64, poi.sample(&mut thread_rng) as i64);
        if v == 0 {
            continue;
        }
        total -= v;
        // println!("{} is from a Poisson(2) distribution", v);
        let mut new_tips = Vec::with_capacity(v as usize);
        let mut futures = Vec::new();
        for _ in 0..v {
            let hash = blockhash::new_unique();
            new_tips.push(hash);

            let b = consensus.build_block_with_parents_and_transactions(hash, tips.clone(), vec![]).to_immutable();

            // Submit to consensus
            let f = consensus.validate_and_insert_block(b);
            futures.push(f);
        }
        join_all(futures).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>().unwrap();
        tips = new_tips;
    }

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = consensus.reachability_store().read().clone_with_new_cache(10000);

    // Assert intervals
    store.validate_intervals(blockhash::ORIGIN).unwrap();

    consensus.shutdown(wait_handles);
}
