use std::{ops::DerefMut, sync::Arc};

use consensus::{
    model::{
        api::hash::{Hash, HashArray},
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore},
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStore},
        },
        ORIGIN,
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
use crossbeam::select;
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
        .add_block(DagBlock::new(1.into(), vec![Hash::ORIGIN]))
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
    store.validate_intervals(Hash::ORIGIN).unwrap();

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
    use crossbeam_channel::unbounded;
    use std::thread;
    let (_tempdir, db) = common::create_temp_db();

    let ghostdag_store = Arc::new(DbGhostdagStore::new(db.clone(), 100000));
    let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), 100000)));
    let relations_store = Arc::new(DbRelationsStore::new(db, 100000));

    let genesis: Hash = 1.into();

    {
        let mut write_guard = reachability_store.write();
        inquirer::init(write_guard.deref_mut()).unwrap();
    }

    let manager = GhostdagManager::new(
        genesis,
        7,
        Arc::clone(&ghostdag_store),
        Arc::clone(&relations_store),
        Arc::new(MTReachabilityService::new(reachability_store.clone())),
    );

    let mut ctx = HeaderProcessingContext::new(genesis);
    manager.init(&mut ctx);
    if let Some(data) = ctx.staged_ghostdag_data {
        ghostdag_store.insert(ctx.hash, data).unwrap();
    }

    let (s, r) = unbounded();
    let reachability_store_clone = reachability_store.clone();

    // Spawn an asynchronous reachability processor.
    let handle = thread::spawn(move || loop {
        select! {
            recv(r) -> data => {
                let ctx: HeaderProcessingContext = data.unwrap();
                if let Some(data) = ctx.staged_ghostdag_data {
                    let mut staging = StagingReachabilityStore::new(reachability_store_clone.upgradable_read());
                    // Add block to staging
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
            }
        }
    });

    let blocks = vec![
        DagBlock::new(1.into(), vec![ORIGIN]),
        DagBlock::new(2.into(), vec![1.into()]),
        DagBlock::new(3.into(), vec![1.into()]),
        DagBlock::new(4.into(), vec![2.into(), 3.into()]),
        DagBlock::new(5.into(), vec![4.into()]),
        DagBlock::new(6.into(), vec![1.into()]),
        DagBlock::new(7.into(), vec![5.into(), 6.into()]),
        DagBlock::new(8.into(), vec![1.into()]),
        DagBlock::new(9.into(), vec![1.into()]),
        DagBlock::new(10.into(), vec![7.into(), 8.into(), 9.into()]),
        DagBlock::new(11.into(), vec![1.into()]),
        DagBlock::new(12.into(), vec![11.into(), 10.into()]),
    ];

    let mut ctx = HeaderProcessingContext::new(genesis);
    manager.init(&mut ctx);
    if let Some(data) = ctx.staged_ghostdag_data.clone() {
        ghostdag_store.insert(ctx.hash, data).unwrap();
        s.send(ctx).unwrap();
    }

    for block in blocks.iter().skip(1).cloned() {
        // Write parents (should be a stage)
        relations_store
            .set_parents(block.hash, HashArray::new(block.parents))
            .unwrap();
        // Create context
        let mut ctx = HeaderProcessingContext::new(block.hash);
        // Add the bock to GHOSTDAG
        manager.add_block(&mut ctx, block.hash);
        // Commit staged GHOSTDAG data
        if let Some(data) = ctx.staged_ghostdag_data.clone() {
            ghostdag_store
                .insert(ctx.hash, data.clone())
                .unwrap();
        }
        // Send to reachability processor
        s.send(ctx).unwrap();
    }

    handle.join().unwrap();

    // Clone with a new cache in order to verify correct writes to the DB itself
    let store = reachability_store
        .read()
        .clone_with_new_cache(10000);

    // Assert intervals
    store.validate_intervals(Hash::ORIGIN).unwrap();

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
