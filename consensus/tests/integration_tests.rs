//!
//! Integration tests
//!

use consensus::misc::uint256::Uint256;
use consensus::model::api::hash::{Hash, HashArray};
use consensus::model::stores::ghostdag::{GhostdagStore, MemoryGhostdagStore};
use consensus::model::stores::reachability::{MemoryReachabilityStore, ReachabilityStore};
use consensus::model::stores::relations::{MemoryRelationsStore, RelationsStore};
use consensus::model::ORIGIN;
use consensus::processes::ghostdag::protocol::{GhostdagManager, StoreAccess};
use consensus::processes::reachability::inquirer;
use consensus::processes::reachability::tests::{validate_intervals, TreeBuilder};

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::rc::Rc;

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

struct StoreAccessImpl {
    ghostdag_store_impl: MemoryGhostdagStore,
    relations_store_impl: MemoryRelationsStore,
    reachability_store_impl: MemoryReachabilityStore,
}

impl StoreAccess<MemoryGhostdagStore, MemoryRelationsStore, MemoryReachabilityStore> for StoreAccessImpl {
    fn relations_store(&self) -> &MemoryRelationsStore {
        &self.relations_store_impl
    }

    fn reachability_store(&self) -> &MemoryReachabilityStore {
        &self.reachability_store_impl
    }

    fn reachability_store_as_mut(&mut self) -> &mut MemoryReachabilityStore {
        &mut self.reachability_store_impl
    }

    fn ghostdag_store_as_mut(&mut self) -> &mut MemoryGhostdagStore {
        &mut self.ghostdag_store_impl
    }

    fn ghostdag_store(&self) -> &MemoryGhostdagStore {
        &self.ghostdag_store_impl
    }
}

#[test]
fn ghostdag_sanity_test() {
    let mut reachability_store = MemoryReachabilityStore::new();
    // let mut builder = TreeBuilder::new_with_params(&mut reachability_store, 2, 5);
    // builder.init_default();

    inquirer::init(&mut reachability_store).unwrap();

    let genesis: Hash = 1.into();
    let genesis_child: Hash = 2.into();

    inquirer::add_block(&mut reachability_store, genesis, ORIGIN, &mut std::iter::empty()).unwrap();

    // builder.add_block(genesis, ORIGIN);
    // builder.add_block(genesis_child, genesis);

    let mut relations_store = MemoryRelationsStore::new();
    relations_store.set_parents(genesis_child, Rc::new(vec![genesis]));

    let mut sa = StoreAccessImpl {
        ghostdag_store_impl: MemoryGhostdagStore::new(),
        relations_store_impl: relations_store,
        reachability_store_impl: reachability_store,
    };

    sa.ghostdag_store_as_mut()
        .set_blue_score(genesis, 0)
        .unwrap();
    sa.ghostdag_store_as_mut()
        .set_blue_work(genesis, Uint256::from_u64(0))
        .unwrap();
    sa.ghostdag_store_as_mut()
        .set_selected_parent(genesis, ORIGIN)
        .unwrap();
    sa.ghostdag_store_as_mut()
        .set_merge_set_blues(genesis, HashArray::new(Vec::new()))
        .unwrap();
    sa.ghostdag_store_as_mut()
        .set_merge_set_reds(genesis, HashArray::new(Vec::new()))
        .unwrap();
    sa.ghostdag_store_as_mut()
        .set_blues_anticone_sizes(genesis, Rc::new(HashMap::new()))
        .unwrap();

    let manager = GhostdagManager::new(genesis, 18);
    manager.add_block(&mut sa, genesis_child);
}

#[derive(Serialize, Deserialize, Debug)]
struct GhostdagTestDag {
    #[serde(rename = "K")]
    k: u8,

    #[serde(rename = "GenesisID")]
    genesis_id: String,

    #[serde(rename = "Blocks")]
    blocks: Vec<GhostdagTestBlock>,
}

#[derive(Serialize, Deserialize, Debug)]
struct GhostdagTestBlock {
    #[serde(rename = "ID")]
    id: String,

    #[serde(rename = "ExpectedScore")]
    score: u64,

    #[serde(rename = "ExpectedSelectedParent")]
    selected_parent: String,

    #[serde(rename = "ExpectedReds")]
    merge_set_reds: Vec<String>,

    #[serde(rename = "ExpectedBlues")]
    merge_set_blues: Vec<String>,

    #[serde(rename = "Parents")]
    parents: Vec<String>,
}

#[test]
fn ghostdag_test() {
    let mut path_strings: Vec<String> = fs::read_dir("tests/testdata/dags")
        .unwrap()
        .map(|f| f.unwrap().path().to_str().unwrap().to_owned())
        .collect();
    path_strings.sort();

    for path_string in path_strings.iter() {
        println!("Running test {}", path_string);
        let path = Path::new(&path_string);
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        let test: GhostdagTestDag = serde_json::from_reader(reader).unwrap();

        let mut reachability_store = MemoryReachabilityStore::new();

        inquirer::init(&mut reachability_store).unwrap();

        let genesis: Hash = string_to_hash(&test.genesis_id);
        inquirer::add_block(&mut reachability_store, genesis, ORIGIN, &mut std::iter::empty()).unwrap();

        let mut relations_store = MemoryRelationsStore::new();
        let mut ghostdag_store = MemoryGhostdagStore::new();

        ghostdag_store.set_blue_score(genesis, 0).unwrap();
        ghostdag_store
            .set_blue_work(genesis, Uint256::from_u64(0))
            .unwrap();
        ghostdag_store
            .set_selected_parent(genesis, ORIGIN)
            .unwrap();
        ghostdag_store
            .set_merge_set_blues(genesis, HashArray::new(Vec::new()))
            .unwrap();
        ghostdag_store
            .set_merge_set_reds(genesis, HashArray::new(Vec::new()))
            .unwrap();
        ghostdag_store
            .set_blues_anticone_sizes(genesis, Rc::new(HashMap::new()))
            .unwrap();

        for block in &test.blocks {
            let block_id = string_to_hash(&block.id);
            let parents = strings_to_hashes(&block.parents);
            relations_store.set_parents(block_id, Rc::clone(&parents));
            // builder.add_block(block_id, parents[0]);
        }

        let mut sa = StoreAccessImpl {
            ghostdag_store_impl: ghostdag_store,
            relations_store_impl: relations_store,
            reachability_store_impl: reachability_store,
        };

        let manager = GhostdagManager::new(genesis, test.k);
        for block in test.blocks {
            println!("Processing block {}", block.id);
            let block_id = string_to_hash(&block.id);
            manager.add_block(&mut sa, block_id);

            assert_eq!(
                sa.ghostdag_store()
                    .get_selected_parent(block_id, false)
                    .unwrap(),
                string_to_hash(&block.selected_parent),
                "selected parent assertion failed for {}",
                block.id,
            );

            assert_eq!(
                sa.ghostdag_store()
                    .get_merge_set_reds(block_id, false)
                    .unwrap(),
                strings_to_hashes(&block.merge_set_reds),
                "mergeset reds assertion failed for {}",
                block.id,
            );

            assert_eq!(
                sa.ghostdag_store()
                    .get_merge_set_blues(block_id, false)
                    .unwrap(),
                strings_to_hashes(&block.merge_set_blues),
                "mergeset blues assertion failed for {:?} with SP {:?}",
                string_to_hash(&block.id),
                string_to_hash(&block.selected_parent)
            );

            assert_eq!(
                sa.ghostdag_store()
                    .get_blue_score(block_id, false)
                    .unwrap(),
                block.score,
                "blue score assertion failed for {}",
                block.id,
            );
        }
    }
}

fn string_to_hash(s: &str) -> Hash {
    let mut data = s.as_bytes().to_vec();
    data.resize(32, 0);
    Hash::new(&data)
}

fn strings_to_hashes(strings: &Vec<String>) -> HashArray {
    let mut arr = Vec::with_capacity(strings.len());
    for string in strings {
        arr.push(string_to_hash(&string));
    }
    Rc::new(arr)
}
