use consensus::{
    model::{api::hash::Hash, stores::reachability::DbReachabilityStore},
    processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions},
};

#[test]
fn test() {
    // Arrange
    let db_tempdir = tempfile::tempdir().unwrap();
    let store = DbReachabilityStore::new(db_tempdir.path().to_str().unwrap(), 10000);
    let mut staging = store.new_staging();

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

    // Reopen the store with a new cache
    let store = store.new_from(10000);

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
