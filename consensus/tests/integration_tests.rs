//!
//! Integration tests
//!

extern crate consensus;

use consensus::model::api::hash::Hash;
use consensus::model::stores::reachability::{MemoryReachabilityStore, ReachabilityStore};
use consensus::processes::reachability::interval::Interval;
use consensus::processes::reachability::tests::TreeBuilder;

#[test]
fn reachability_test() {
    // Arrange
    let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());

    // Act
    let root: Hash = 1.into();
    let mut builder = TreeBuilder::new_with_params(store.as_mut(), 2, 5);
    builder.init(root, Interval::maximal());
    for i in 2u64..100 {
        builder.add_block(i.into(), (i / 2).into());
    }

    // Should trigger an earlier than reindex root allocation
    builder.add_block(100.into(), 2.into());
}
