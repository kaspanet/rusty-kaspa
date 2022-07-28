//!
//! Integration tests
//!

use consensus::model::api::hash::Hash;
use consensus::model::stores::reachability::{MemoryReachabilityStore, ReachabilityStore};
use consensus::processes::reachability::interval::Interval;
use consensus::processes::reachability::tests::{validate_intervals, TreeBuilder};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
struct Block {
    id: String,
    parents: Vec<String>,
}

#[test]
fn reachability_stretch_test() {
    // Arrange
    let json_path = Path::new("tests/testdata/reachability/noattack-dag-blocks--2^12-delay-factor--1-k--18.json.gz");
    let file = File::open(json_path).unwrap();
    let decoder = GzDecoder::new(file);
    let blocks: Vec<Block> = serde_json::from_reader(decoder).unwrap();

    // Prepare block data
    let root: Hash = Hash::ORIGIN;
    let mut map = HashMap::<String, Hash>::new();
    for block in blocks.iter().skip(1) {
        map.insert(block.id.clone(), block.id.parse::<u64>().unwrap().into());
    }
    // "0" is an illegal hash, so we map it to root
    map.insert("0".to_owned(), root);

    // Act
    let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());
    let mut builder = TreeBuilder::new_with_params(store.as_mut(), 2, 5);
    builder.init(root, Interval::maximal());

    for (i, block) in blocks.iter().skip(1).enumerate() {
        // For now, choose the first parent as selected
        builder.add_block(map[&block.id], map[&block.parents[0]]);
        if i % 10 == 0 {
            validate_intervals(*builder.store(), root).unwrap();
        }
    }

    // Assert
    validate_intervals(store.as_ref(), root).unwrap();
}

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
    validate_intervals(store.as_ref(), root).unwrap();
}
