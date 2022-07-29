//!
//! Integration tests
//!

use consensus::model::api::hash::Hash;
use consensus::model::stores::reachability::{MemoryReachabilityStore, ReachabilityStore};
use consensus::processes::reachability::tests::{validate_intervals, TreeBuilder};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Serialize, Deserialize, Debug)]
struct JsonBlock {
    id: String,
    parents: Vec<String>,
}

#[derive(Clone)]
struct DagBlock {
    hash: Hash,
    parents: Vec<Hash>,
}

impl From<&JsonBlock> for DagBlock {
    fn from(src: &JsonBlock) -> Self {
        // Apply +1 to ids to avoid the zero hash
        Self::new(
            (src.id.parse::<u64>().unwrap() + 1).into(),
            src.parents
                .iter()
                .map(|id| (id.parse::<u64>().unwrap() + 1).into())
                .collect(),
        )
    }
}

impl DagBlock {
    fn new(hash: Hash, parents: Vec<Hash>) -> Self {
        Self { hash, parents }
    }
}

// Test configuration
const NUM_BLOCKS_EXPONENT: i32 = 12;

fn reachability_stretch_test(use_attack_json: bool) {
    // Arrange
    let path_str = format!(
        "tests/testdata/reachability/{}attack-dag-blocks--2^{}-delay-factor--1-k--18.json.gz",
        if use_attack_json { "" } else { "no" },
        NUM_BLOCKS_EXPONENT
    );
    let path = Path::new(&path_str);
    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);
    let decoder = GzDecoder::new(reader);
    let json_blocks: Vec<JsonBlock> = serde_json::from_reader(decoder).unwrap();

    let mut map = HashMap::<Hash, DagBlock>::new();
    let mut blocks = Vec::<Hash>::new();
    for json_block in &json_blocks {
        let block: DagBlock = json_block.into();
        blocks.push(block.hash);
        map.insert(block.hash, block);
    }

    // Act
    let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());
    let mut builder = TreeBuilder::new_with_params(store.as_mut(), 2, 5);

    let root = consensus::model::ORIGIN;
    builder.init_default();

    for (i, block) in blocks.iter().enumerate() {
        // For now, choose the first parent as selected
        let parent = map[block].parents.first().unwrap_or(&root);
        builder.add_block(*block, *parent);
        if i % 10 == 0 {
            validate_intervals(*builder.store(), root).unwrap();
        }
    }
    validate_intervals(*builder.store(), root).unwrap();

    let num_chains = blocks.len() / 2;
    let max_chain = 20;
    let validation_freq = usize::max(1, num_chains / 100);

    use rand::prelude::*;
    let mut rng = StdRng::seed_from_u64(22322);

    for i in 0..num_chains {
        let rand_idx = rng.gen_range(0..blocks.len());
        let rand_parent = blocks[rand_idx];
        let new_block: Hash = ((blocks.len() + 1) as u64).into();
        builder.add_block(new_block, rand_parent);
        blocks.push(new_block);
        map.insert(new_block, DagBlock { hash: new_block, parents: vec![rand_parent] });

        // Add a random-length chain with probability 1/8
        if rng.gen_range(0..8) == 0 {
            let chain_len = rng.gen_range(0..max_chain);
            let mut chain_tip = new_block;
            for _ in 0..chain_len {
                let new_block: Hash = ((blocks.len() + 1) as u64).into();
                builder.add_block(new_block, chain_tip);
                blocks.push(new_block);
                map.insert(new_block, DagBlock { hash: new_block, parents: vec![chain_tip] });
                chain_tip = new_block;
            }
        }

        if cfg!(debug_assertions) {
            if i % validation_freq == 0 || i == num_chains - 1 {
                validate_intervals(*builder.store(), root).unwrap();
            } else {
                // In debug mode and for most iterations, validate intervals for
                // new chain only in order to shorten the test
                validate_intervals(*builder.store(), new_block).unwrap();
            }
        } else {
            validate_intervals(*builder.store(), root).unwrap();
        }
    }

    // Assert
    validate_intervals(store.as_ref(), root).unwrap();
}

#[test]
fn test_attack_json() {
    reachability_stretch_test(true);
}

#[test]
fn test_noattack_json() {
    reachability_stretch_test(false);
}
