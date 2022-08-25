//!
//! Integration tests
//!

use consensus::consensus::test_consensus::TestConsensus;
use consensus::model::stores::ghostdag::{GhostdagStoreReader, KType as GhostdagKType};
use consensus::model::stores::reachability::DbReachabilityStore;
use consensus::params::MAINNET_PARAMS;
use consensus::processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions};
use consensus_core::block::Block;
use consensus_core::blockhash;
use hashes::Hash;

use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;

mod common;

#[derive(Serialize, Deserialize, Debug)]
struct JsonBlock {
    id: String,
    parents: Vec<String>,
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

    let root = blockhash::ORIGIN;
    let mut map = HashMap::<Hash, DagBlock>::new();
    let mut blocks = Vec::<Hash>::new();

    for json_block in &json_blocks {
        let block: DagBlock = json_block.into();
        blocks.push(block.hash);
        map.insert(block.hash, block);
    }
    // Set root as genesis parent
    map.get_mut(&blocks[0])
        .unwrap()
        .parents
        .push(root);

    // Act
    let (_tempdir, db) = common::create_temp_db();
    let mut store = DbReachabilityStore::new(db, 100000);
    let mut builder = DagBuilder::new(&mut store);

    builder.init();

    for (i, block) in blocks.iter().enumerate() {
        builder.add_block(map[block].clone());
        if i % 100 == 0 {
            builder.store().validate_intervals(root).unwrap();
        }
    }
    builder.store().validate_intervals(root).unwrap();

    let num_chains = if use_attack_json { blocks.len() / 8 } else { blocks.len() / 2 };
    let max_chain = 20;
    let validation_freq = usize::max(1, num_chains / 100);

    use rand::prelude::*;
    let mut rng = StdRng::seed_from_u64(22322);

    for i in 0..num_chains {
        let rand_idx = rng.gen_range(0..blocks.len());
        let rand_parent = blocks[rand_idx];
        let new_hash: Hash = ((blocks.len() + 1) as u64).into();
        let new_block = DagBlock::new(new_hash, vec![rand_parent]);
        builder.add_block(new_block.clone());
        blocks.push(new_hash);
        map.insert(new_hash, new_block);

        // Add a random-length chain with probability 1/8
        if rng.gen_range(0..8) == 0 {
            let chain_len = rng.gen_range(0..max_chain);
            let mut chain_tip = new_hash;
            for _ in 0..chain_len {
                let new_hash: Hash = ((blocks.len() + 1) as u64).into();
                let new_block = DagBlock::new(new_hash, vec![chain_tip]);
                builder.add_block(new_block.clone());
                blocks.push(new_hash);
                map.insert(new_hash, new_block);
                chain_tip = new_hash;
            }
        }

        if i % validation_freq == 0 {
            builder.store().validate_intervals(root).unwrap();
        } else {
            // For most iterations, validate intervals for
            // new chain only in order to shorten the test
            builder
                .store()
                .validate_intervals(new_hash)
                .unwrap();
        }
    }

    // Assert
    store.validate_intervals(root).unwrap();
}

#[test]
fn test_attack_json() {
    reachability_stretch_test(true);
}

#[test]
fn test_noattack_json() {
    reachability_stretch_test(false);
}

#[test]
fn consensus_sanity_test() {
    let genesis_child: Hash = 2.into();

    let (_tempdir, db) = common::create_temp_db();
    let consensus = TestConsensus::new(db, &MAINNET_PARAMS);
    let wait_handle = consensus.init();

    let _ = consensus.validate_and_insert_block(Arc::new(
        consensus.build_block_with_parents(genesis_child, vec![MAINNET_PARAMS.genesis_hash]),
    ));
    let (_, _) = consensus.drop();
    wait_handle.join().unwrap();
}

#[derive(Serialize, Deserialize, Debug)]
struct GhostdagTestDag {
    #[serde(rename = "K")]
    k: GhostdagKType,

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
    mergeset_reds: Vec<String>,

    #[serde(rename = "ExpectedBlues")]
    mergeset_blues: Vec<String>,

    #[serde(rename = "Parents")]
    parents: Vec<String>,
}

#[tokio::test]
async fn ghostdag_test() {
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

        let (_tempdir, db) = common::create_temp_db();

        let mut params = MAINNET_PARAMS;
        params.genesis_hash = string_to_hash(&test.genesis_id);
        params.ghostdag_k = test.k;

        let consensus = TestConsensus::new(db, &params);
        let wait_handle = consensus.init();

        for block in test.blocks.iter() {
            println!("Processing block {}", block.id);
            let block_id = string_to_hash(&block.id);
            let block_header = consensus.build_header_with_parents(block_id, strings_to_hashes(&block.parents));

            // Submit to consensus
            consensus
                .validate_and_insert_block(Arc::new(Block::from_header(block_header)))
                .await
                .unwrap();
        }

        let (_, ghostdag_store) = consensus.drop();

        // Clone with a new cache in order to verify correct writes to the DB itself
        let ghostdag_store = ghostdag_store.clone_with_new_cache(10000);

        // Wait for async consensus processors to exit
        wait_handle.join().unwrap();

        // Assert GHOSTDAG output data
        for block in test.blocks {
            println!("Asserting block {}", block.id);
            let block_id = string_to_hash(&block.id);
            let output_ghostdag_data = ghostdag_store.get_data(block_id).unwrap();

            assert_eq!(
                output_ghostdag_data.selected_parent,
                string_to_hash(&block.selected_parent),
                "selected parent assertion failed for {}",
                block.id,
            );

            assert_eq!(
                output_ghostdag_data.mergeset_reds.to_vec(),
                strings_to_hashes(&block.mergeset_reds),
                "mergeset reds assertion failed for {}",
                block.id,
            );

            assert_eq!(
                output_ghostdag_data.mergeset_blues.to_vec(),
                strings_to_hashes(&block.mergeset_blues),
                "mergeset blues assertion failed for {:?} with SP {:?}",
                string_to_hash(&block.id),
                string_to_hash(&block.selected_parent)
            );

            assert_eq!(output_ghostdag_data.blue_score, block.score, "blue score assertion failed for {}", block.id,);
        }
    }
}

fn string_to_hash(s: &str) -> Hash {
    let mut data = s.as_bytes().to_vec();
    data.resize(32, 0);
    Hash::from_slice(&data)
}

fn strings_to_hashes(strings: &Vec<String>) -> Vec<Hash> {
    let mut vec = Vec::with_capacity(strings.len());
    for string in strings {
        vec.push(string_to_hash(string));
    }
    vec
}
