use consensus::{
    model::{
        api::hash::Hash,
        stores::reachability::{DbReachabilityStore, StagingReachabilityStore},
    },
    processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions},
};
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

    // let (_tempdir, db) = common::create_temp_db();

    // let ghostdag_store = DbGhostdagStore::new(db.clone(), 100000);
    // let mut reachability_store = DbReachabilityStore::new(db.clone(), 100000);
    // let relations_store = DbRelationsStore::new(db, 100000);

    // let genesis: Hash = 1.into();

    // inquirer::init(&mut reachability_store).unwrap();
    // inquirer::add_block(&mut reachability_store, genesis, ORIGIN, &mut std::iter::empty()).unwrap();

    // let mut stores =
    //     StoreAccessDbImpl::new(ghostdag_store, relations_store, STReachabilityService::new(reachability_store));
    // let manager = GhostdagManager::new(genesis, 6);
    // manager.init(&mut stores);

    let (s, r) = unbounded();

    // Computes the n-th Fibonacci number.
    fn fib(n: i32) -> i32 {
        if n <= 1 {
            n
        } else {
            fib(n - 1) + fib(n - 2)
        }
    }

    // Spawn an asynchronous computation.
    thread::spawn(move || s.send(fib(20)).unwrap());

    // Print the result of the computation.
    println!("{}", r.recv().unwrap());
}
