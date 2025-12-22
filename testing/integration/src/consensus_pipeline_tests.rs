use futures_util::future::try_join_all;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::{
    config::ConfigBuilder, consensus::test_consensus::TestConsensus, params::MAINNET_PARAMS,
    processes::reachability::tests::StoreValidationExtensions,
};
use kaspa_consensus_core::{api::ConsensusApi, blockhash};
use kaspa_database::prelude::CachePolicy;
use kaspa_hashes::Hash;
use rand_distr::{Distribution, Poisson};
use std::cmp::min;
use tokio::join;

#[tokio::test]
async fn test_concurrent_pipeline() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.genesis.hash = 1.into()).build();
    let consensus = TestConsensus::new(&config);
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
        let b: kaspa_consensus_core::block::Block = consensus.build_header_only_block_with_parents(hash, parents).to_immutable();
        let results = join!(
            consensus.validate_and_insert_block(b.clone()).virtual_state_task,
            consensus.validate_and_insert_block(b).virtual_state_task
        );
        results.0.unwrap();
        results.1.unwrap();
    }

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = consensus.reachability_store().read().clone_with_new_cache(CachePolicy::Count(10_000), CachePolicy::Count(10_000));

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
    init_allocator_with_default_settings();
    let genesis: Hash = blockhash::new_unique();
    let bps = 8;
    let delay = 2;

    let poi = Poisson::new((bps * delay) as f64).unwrap();
    let mut thread_rng = rand::thread_rng();

    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().edit_consensus_params(|p| p.genesis.hash = genesis).build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let mut tips = vec![genesis];
    let mut total = 1000i64;
    while total > 0 {
        let v = min(config.max_block_parents() as i64, poi.sample(&mut thread_rng) as i64);
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
            let f = consensus.validate_and_insert_block(b).virtual_state_task;
            futures.push(f);
        }
        try_join_all(futures).await.unwrap();
        tips = new_tips;
    }

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = consensus.reachability_store().read().clone_with_new_cache(CachePolicy::Count(10_000), CachePolicy::Count(10_000));

    // Assert intervals
    store.validate_intervals(blockhash::ORIGIN).unwrap();

    consensus.shutdown(wait_handles);
}
