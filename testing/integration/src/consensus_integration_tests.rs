//!
//! Integration tests
//!

use async_channel::unbounded;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_consensus::config::genesis::GENESIS;
use kaspa_consensus::config::{Config, ConfigBuilder};
use kaspa_consensus::consensus::factory::Factory as ConsensusFactory;
use kaspa_consensus::consensus::test_consensus::{TestConsensus, TestConsensusFactory};
use kaspa_consensus::model::stores::block_transactions::{
    BlockTransactionsStore, BlockTransactionsStoreReader, DbBlockTransactionsStore,
};
use kaspa_consensus::model::stores::ghostdag::{GhostdagStoreReader, KType as GhostdagKType};
use kaspa_consensus::model::stores::headers::HeaderStoreReader;
use kaspa_consensus::model::stores::reachability::DbReachabilityStore;
use kaspa_consensus::model::stores::relations::DbRelationsStore;
use kaspa_consensus::model::stores::selected_chain::SelectedChainStoreReader;
use kaspa_consensus::params::{
    ForkActivation, Params, CRESCENDO, DEVNET_PARAMS, MAINNET_PARAMS, MAX_DIFFICULTY_TARGET, MAX_DIFFICULTY_TARGET_AS_F64,
};
use kaspa_consensus::pipeline::monitor::ConsensusMonitor;
use kaspa_consensus::pipeline::ProcessingCounters;
use kaspa_consensus::processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions};
use kaspa_consensus::processes::window::{WindowManager, WindowType};
use kaspa_consensus_core::api::args::TransactionValidationArgs;
use kaspa_consensus_core::api::{BlockValidationFutures, ConsensusApi};
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::blockhash::new_unique;
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::constants::{BLOCK_VERSION, SOMPI_PER_KASPA, STORAGE_MASS_PARAMETER, TRANSIENT_BYTE_TO_MASS_FACTOR};
use kaspa_consensus_core::errors::block::{BlockProcessResult, RuleError};
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::mining_rules::MiningRules;
use kaspa_consensus_core::network::{NetworkId, NetworkType::Mainnet};
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::trusted::{ExternalGhostdagData, TrustedBlock};
use kaspa_consensus_core::tx::{
    MutableTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_consensus_core::{blockhash, hashing, BlockHashMap, BlueWorkType};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensus_notify::service::NotifyService;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::tick::TickService;
use kaspa_core::time::unix_now;
use kaspa_database::utils::get_kaspa_tempdir;
use kaspa_hashes::Hash;
use kaspa_utils::arc::ArcExtensions;

use crate::common;
use flate2::read::GzDecoder;
use futures_util::future::try_join_all;
use itertools::Itertools;
use kaspa_consensus_core::errors::tx::TxRuleError;
use kaspa_consensus_core::hashing::sighash::calc_schnorr_signature_hash;
use kaspa_consensus_core::merkle::calc_hash_merkle_root;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_core::core::Core;
use kaspa_core::signals::Shutdown;
use kaspa_core::task::runtime::AsyncRuntime;
use kaspa_core::{assert_match, info};
use kaspa_database::create_temp_db;
use kaspa_database::prelude::{CachePolicy, ConnBuilder};
use kaspa_index_processor::service::IndexService;
use kaspa_math::Uint256;
use kaspa_muhash::MuHash;
use kaspa_notify::subscription::context::SubscriptionContext;
use kaspa_txscript::caches::TxScriptCacheCounters;
use kaspa_txscript::opcodes::codes::OpTrue;
use kaspa_txscript::script_builder::ScriptBuilderResult;
use kaspa_utxoindex::api::{UtxoIndexApi, UtxoIndexProxy};
use kaspa_utxoindex::UtxoIndex;
use serde::{Deserialize, Serialize};
use std::cmp::{max, Ordering};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use std::{
    collections::HashMap,
    fs::File,
    future::Future,
    io::{BufRead, BufReader},
    str::{from_utf8, FromStr},
};

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
            src.parents.iter().map(|id| (id.parse::<u64>().unwrap() + 1).into()).collect(),
        )
    }
}

// Test configuration
const NUM_BLOCKS_EXPONENT: i32 = 12;

fn reachability_stretch_test(use_attack_json: bool) {
    // Arrange
    let path_str = format!(
        "testdata/reachability/{}attack-dag-blocks--2^{}-delay-factor--1-k--18.json.gz",
        if use_attack_json { "" } else { "no" },
        NUM_BLOCKS_EXPONENT
    );
    let file = common::open_file(Path::new(&path_str));
    let decoder = GzDecoder::new(file);
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
    map.get_mut(&blocks[0]).unwrap().parents.push(root);

    // Act
    let (_temp_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
    let mut store = DbReachabilityStore::new(db.clone(), CachePolicy::Count(50_000), CachePolicy::Count(50_000));
    let mut relations = DbRelationsStore::new(db, 0, CachePolicy::Count(100_000), CachePolicy::Count(100_000)); // TODO: remove level
    let mut builder = DagBuilder::new(&mut store, &mut relations);

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
    let mut rng: StdRng = StdRng::seed_from_u64(22322);

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
            builder.store().validate_intervals(new_hash).unwrap();
        }
    }

    // Assert
    store.validate_intervals(root).unwrap();
}

#[test]
fn test_attack_json() {
    init_allocator_with_default_settings();
    reachability_stretch_test(true);
}

#[test]
fn test_noattack_json() {
    init_allocator_with_default_settings();
    reachability_stretch_test(false);
}

#[tokio::test]
async fn consensus_sanity_test() {
    init_allocator_with_default_settings();
    let genesis_child: Hash = 2.into();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    consensus
        .validate_and_insert_block(consensus.build_block_with_parents(genesis_child, vec![MAINNET_PARAMS.genesis.hash]).to_immutable())
        .virtual_state_task
        .await
        .unwrap();

    consensus.shutdown(wait_handles);
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
    init_allocator_with_default_settings();
    let mut path_strings: Vec<String> =
        common::read_dir("testdata/dags").map(|f| f.unwrap().path().to_str().unwrap().to_owned()).collect();
    path_strings.sort();

    for path_str in path_strings.iter() {
        info!("Running test {path_str}");
        let file = File::open(path_str).unwrap();
        let reader = BufReader::new(file);
        let test: GhostdagTestDag = serde_json::from_reader(reader).unwrap();

        let config = ConfigBuilder::new(MAINNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| {
                p.genesis.hash = string_to_hash(&test.genesis_id);
                p.prior_ghostdag_k = test.k;
                p.min_difficulty_window_size = p.prior_difficulty_window_size;
            })
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();

        for block in test.blocks.iter() {
            info!("Processing block {}", block.id);
            let block_id = string_to_hash(&block.id);
            let block_header = consensus.build_header_with_parents(block_id, strings_to_hashes(&block.parents));

            // Submit to consensus
            consensus.validate_and_insert_block(Block::from_header(block_header)).virtual_state_task.await.unwrap();
        }

        // Clone with a new cache in order to verify correct writes to the DB itself
        let ghostdag_store = consensus.ghostdag_store().clone_with_new_cache(CachePolicy::Count(10_000), CachePolicy::Count(10_000));

        // Assert GHOSTDAG output data
        for block in test.blocks {
            info!("Asserting block {}", block.id);
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

        consensus.shutdown(wait_handles);
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

#[tokio::test]
async fn block_window_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.genesis.hash = string_to_hash("A");
            p.prior_ghostdag_k = 1;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    struct TestBlock {
        parents: Vec<&'static str>,
        id: &'static str,
        expected_window: Vec<&'static str>,
    }

    let test_blocks = vec![
        TestBlock { parents: vec!["A"], id: "B", expected_window: vec![] },
        TestBlock { parents: vec!["B"], id: "C", expected_window: vec!["B"] },
        TestBlock { parents: vec!["B"], id: "D", expected_window: vec!["B"] },
        TestBlock { parents: vec!["D", "C"], id: "E", expected_window: vec!["D", "C", "B"] },
        TestBlock { parents: vec!["D", "C"], id: "F", expected_window: vec!["D", "C", "B"] },
        TestBlock { parents: vec!["A"], id: "G", expected_window: vec![] },
        TestBlock { parents: vec!["G"], id: "H", expected_window: vec!["G"] },
        TestBlock { parents: vec!["H", "F"], id: "I", expected_window: vec!["F", "H", "D", "C", "G", "B"] },
        TestBlock { parents: vec!["I"], id: "J", expected_window: vec!["I", "F", "H", "D", "C", "G", "B"] },
        TestBlock { parents: vec!["J"], id: "K", expected_window: vec!["J", "I", "F", "H", "D", "C", "G", "B"] },
        TestBlock { parents: vec!["K"], id: "L", expected_window: vec!["K", "J", "I", "F", "H", "D", "C", "G", "B"] },
        TestBlock { parents: vec!["L"], id: "M", expected_window: vec!["L", "K", "J", "I", "F", "H", "D", "C", "G", "B"] },
        TestBlock { parents: vec!["M"], id: "N", expected_window: vec!["M", "L", "K", "J", "I", "F", "H", "D", "C", "G"] },
        TestBlock { parents: vec!["N"], id: "O", expected_window: vec!["N", "M", "L", "K", "J", "I", "F", "H", "D", "C"] },
    ];

    for test_block in test_blocks {
        info!("Processing block {}", test_block.id);
        let block_id = string_to_hash(test_block.id);
        let block = consensus.build_block_with_parents(
            block_id,
            strings_to_hashes(&test_block.parents.iter().map(|parent| String::from(*parent)).collect()),
        );

        // Submit to consensus
        consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();

        let window = consensus
            .window_manager()
            .block_window(&consensus.ghostdag_store().get_data(block_id).unwrap(), WindowType::VaryingWindow(10))
            .unwrap()
            .blocks
            .clone();

        let window_hashes: Vec<String> = window
            .into_sorted_vec()
            .iter()
            .map(|item| {
                let slice = &item.0.hash.as_bytes()[..1];
                from_utf8(slice).unwrap().to_owned()
            })
            .collect();

        let expected_window_ids: Vec<String> = test_block.expected_window.iter().map(|id| String::from(*id)).collect();
        assert_eq!(expected_window_ids, window_hashes);
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn header_in_isolation_validation_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).edit_consensus_params(|p| p.skip_proof_of_work = true).build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let block = consensus.build_block_with_parents(1.into(), vec![config.genesis.hash]);

    {
        let mut block = block.clone();
        let block_version = BLOCK_VERSION - 1;
        block.header.version = block_version;
        match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
            Err(RuleError::WrongBlockVersion(wrong_version)) => {
                assert_eq!(wrong_version, block_version)
            }
            res => {
                panic!("Unexpected result: {res:?}")
            }
        }
    }

    {
        let mut block = block.clone();
        block.header.hash = 2.into();

        let now = unix_now();
        let block_ts = now + config.timestamp_deviation_tolerance * config.prior_target_time_per_block + 2000;
        block.header.timestamp = block_ts;
        match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
            Err(RuleError::TimeTooFarIntoTheFuture(ts, _)) => {
                assert_eq!(ts, block_ts)
            }
            res => {
                panic!("Unexpected result: {res:?}")
            }
        }
    }

    {
        let mut block = block.clone();
        block.header.hash = 3.into();
        block.header.parents_by_level[0] = vec![];
        match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
            Err(RuleError::NoParents) => {}
            res => {
                panic!("Unexpected result: {res:?}")
            }
        }
    }

    {
        let mut block = block.clone();
        block.header.hash = 4.into();
        block.header.parents_by_level[0] =
            std::iter::repeat(config.genesis.hash).take(config.prior_max_block_parents as usize + 1).collect();
        match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
            Err(RuleError::TooManyParents(num_parents, limit)) => {
                assert_eq!((config.prior_max_block_parents + 1) as usize, num_parents);
                assert_eq!(limit, config.prior_max_block_parents as usize);
            }
            res => {
                panic!("Unexpected result: {res:?}")
            }
        }
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn incest_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let block = consensus.build_block_with_parents(1.into(), vec![config.genesis.hash]);
    let BlockValidationFutures { block_task, virtual_state_task } = consensus.validate_and_insert_block(block.to_immutable());
    block_task.await.unwrap(); // Assert that block task completes as well
    virtual_state_task.await.unwrap();

    let mut block = consensus.build_block_with_parents(2.into(), vec![config.genesis.hash]);
    block.header.parents_by_level[0] = vec![1.into(), config.genesis.hash];
    let BlockValidationFutures { block_task, virtual_state_task } = consensus.validate_and_insert_block(block.to_immutable());
    match virtual_state_task.await {
        Err(RuleError::InvalidParentsRelation(a, b)) => {
            assert_eq!(a, config.genesis.hash);
            assert_eq!(b, 1.into());
            // Assert that block task returns the same error as well
            assert_match!(block_task.await, Err(RuleError::InvalidParentsRelation(_, _)));
        }
        res => {
            panic!("Unexpected result: {res:?}")
        }
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn missing_parents_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let mut block = consensus.build_block_with_parents(1.into(), vec![config.genesis.hash]);
    block.header.parents_by_level[0] = vec![0.into()];
    let BlockValidationFutures { block_task, virtual_state_task } = consensus.validate_and_insert_block(block.to_immutable());
    match virtual_state_task.await {
        Err(RuleError::MissingParents(missing)) => {
            assert_eq!(missing, vec![0.into()]);
            // Assert that block task returns the same error as well
            assert_match!(block_task.await, Err(RuleError::MissingParents(_)));
        }
        res => {
            panic!("Unexpected result: {res:?}")
        }
    }

    consensus.shutdown(wait_handles);
}

// Errors such as ErrTimeTooOld which happen after DAA and PoW validation should set the block
// as a known invalid.
#[tokio::test]
async fn known_invalid_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();
    let mut block = consensus.build_block_with_parents(1.into(), vec![config.genesis.hash]);
    block.header.timestamp -= 1;

    match consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await {
        Err(RuleError::TimeTooOld(_, _)) => {}
        res => {
            panic!("Unexpected result: {res:?}")
        }
    }

    match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
        Err(RuleError::KnownInvalid) => {}
        res => {
            panic!("Unexpected result: {res:?}")
        }
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn median_time_test() {
    init_allocator_with_default_settings();
    struct Test {
        name: &'static str,
        config: Config,
    }

    let tests = vec![
        Test {
            name: "MAINNET with full window",
            config: ConfigBuilder::new(MAINNET_PARAMS)
                .skip_proof_of_work()
                .edit_consensus_params(|p| {
                    p.crescendo_activation = ForkActivation::never();
                })
                .build(),
        },
        Test {
            name: "MAINNET with sampled window",
            config: ConfigBuilder::new(MAINNET_PARAMS)
                .skip_proof_of_work()
                .edit_consensus_params(|p| {
                    p.crescendo_activation = ForkActivation::always();
                    p.timestamp_deviation_tolerance = 120;
                    p.crescendo.past_median_time_sample_rate = 3;
                    p.crescendo.past_median_time_sampled_window_size = (2 * 120 - 1) / 3;
                })
                .build(),
        },
    ];

    for test in tests {
        let consensus = TestConsensus::new(&test.config);
        let wait_handles = consensus.init();

        let num_blocks =
            test.config.past_median_time_window_size().before() as u64 * test.config.past_median_time_sample_rate().before();
        let timestamp_deviation_tolerance = test.config.timestamp_deviation_tolerance;
        for i in 1..(num_blocks + 1) {
            let parent = if i == 1 { test.config.genesis.hash } else { (i - 1).into() };
            let mut block = consensus.build_block_with_parents(i.into(), vec![parent]);
            block.header.timestamp = test.config.genesis.timestamp + i;
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();
        }

        let mut block = consensus.build_block_with_parents((num_blocks + 2).into(), vec![num_blocks.into()]);
        // We set the timestamp to be less than the median time and expect the block to be rejected
        block.header.timestamp = test.config.genesis.timestamp + num_blocks - timestamp_deviation_tolerance - 1;
        match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
            Err(RuleError::TimeTooOld(_, _)) => {}
            res => {
                panic!("{}: Unexpected result: {:?}", test.name, res)
            }
        }

        let mut block = consensus.build_block_with_parents((num_blocks + 3).into(), vec![num_blocks.into()]);
        // We set the timestamp to be the exact median time and expect the block to be rejected
        block.header.timestamp = test.config.genesis.timestamp + num_blocks - timestamp_deviation_tolerance;
        match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
            Err(RuleError::TimeTooOld(_, _)) => {}
            res => {
                panic!("{}: Unexpected result: {:?}", test.name, res)
            }
        }

        let mut block = consensus.build_block_with_parents((num_blocks + 4).into(), vec![(num_blocks).into()]);
        // We set the timestamp to be bigger than the median time and expect the block to be inserted successfully.
        block.header.timestamp = test.config.genesis.timestamp + timestamp_deviation_tolerance + 1;
        consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();

        consensus.shutdown(wait_handles);
    }
}

#[tokio::test]
async fn mergeset_size_limit_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let num_blocks_per_chain = config.prior_mergeset_size_limit + 1;

    let mut tip1_hash = config.genesis.hash;
    for i in 1..(num_blocks_per_chain + 1) {
        let block = consensus.build_block_with_parents(i.into(), vec![tip1_hash]);
        tip1_hash = block.header.hash;
        consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();
    }

    let mut tip2_hash = config.genesis.hash;
    for i in (num_blocks_per_chain + 2)..(2 * num_blocks_per_chain + 1) {
        let block = consensus.build_block_with_parents(i.into(), vec![tip2_hash]);
        tip2_hash = block.header.hash;
        consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();
    }

    let block = consensus.build_block_with_parents((3 * num_blocks_per_chain + 1).into(), vec![tip1_hash, tip2_hash]);
    match consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await {
        Err(RuleError::MergeSetTooBig(a, b)) => {
            assert_eq!(a, config.prior_mergeset_size_limit + 1);
            assert_eq!(b, config.prior_mergeset_size_limit);
        }
        res => {
            panic!("Unexpected result: {res:?}")
        }
    }

    consensus.shutdown(wait_handles);
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCBlock {
    Header: RPCBlockHeader,
    Transactions: Vec<RPCTransaction>,
    VerboseData: RPCBlockVerboseData,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCTransaction {
    Version: u16,
    Inputs: Vec<RPCTransactionInput>,
    Outputs: Vec<RPCTransactionOutput>,
    LockTime: u64,
    SubnetworkID: String,
    Gas: u64,
    Payload: String,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCTransactionOutput {
    Amount: u64,
    ScriptPublicKey: RPCScriptPublicKey,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCScriptPublicKey {
    Version: u16,
    Script: String,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCTransactionInput {
    PreviousOutpoint: RPCOutpoint,
    SignatureScript: String,
    Sequence: u64,
    SigOpCount: u8,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCOutpoint {
    TransactionID: String,
    Index: u32,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCBlockHeader {
    Version: u16,
    Parents: Vec<RPCBlockLevelParents>,
    HashMerkleRoot: String,
    AcceptedIDMerkleRoot: String,
    UTXOCommitment: String,
    Timestamp: u64,
    Bits: u32,
    Nonce: u64,
    DAAScore: u64,
    BlueScore: u64,
    BlueWork: String,
    PruningPoint: String,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCBlockLevelParents {
    ParentHashes: Vec<String>,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCBlockVerboseData {
    Hash: String,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct JsonBlockWithTrustedData {
    Block: RPCBlock,
    GHOSTDAG: JsonGHOSTDAGData,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct JsonGHOSTDAGData {
    BlueScore: u64,
    BlueWork: String,
    SelectedParent: String,
    MergeSetBlues: Vec<String>,
    MergeSetReds: Vec<String>,
    BluesAnticoneSizes: Vec<JsonBluesAnticoneSizes>,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct JsonBluesAnticoneSizes {
    BlueHash: String,
    AnticoneSize: GhostdagKType,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct JsonOutpointUTXOEntryPair {
    Outpoint: RPCOutpoint,
    UTXOEntry: RPCUTXOEntry,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct RPCUTXOEntry {
    Amount: u64,
    ScriptPublicKey: RPCScriptPublicKey,
    BlockDAAScore: u64,
    IsCoinbase: bool,
}

#[allow(non_snake_case)]
#[derive(Deserialize, Debug)]
struct KaspadGoParams {
    K: GhostdagKType,
    TimestampDeviationTolerance: u64,
    TargetTimePerBlock: u64,
    MaxBlockParents: u8,
    DifficultyAdjustmentWindowSize: usize,
    MergeSetSizeLimit: u64,
    MergeDepth: u64,
    FinalityDuration: u64,
    CoinbasePayloadScriptPublicKeyMaxLength: u8,
    MaxCoinbasePayloadLength: usize,
    MassPerTxByte: u64,
    MassPerSigOp: u64,
    MassPerScriptPubKeyByte: u64,
    MaxBlockMass: u64,
    DeflationaryPhaseDaaScore: u64,
    PreDeflationaryPhaseBaseSubsidy: u64,
    SkipProofOfWork: bool,
    MaxBlockLevel: u8,
    PruningProofM: u64,
}

impl KaspadGoParams {
    fn into_params(self) -> Params {
        let finality_depth = self.FinalityDuration / self.TargetTimePerBlock;
        Params {
            dns_seeders: &[],
            net: NetworkId { network_type: Mainnet, suffix: None },
            genesis: GENESIS,
            prior_ghostdag_k: self.K,
            timestamp_deviation_tolerance: self.TimestampDeviationTolerance,
            prior_target_time_per_block: self.TargetTimePerBlock / 1_000_000,
            prior_max_block_parents: self.MaxBlockParents,
            max_difficulty_target: MAX_DIFFICULTY_TARGET,
            max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
            prior_difficulty_window_size: self.DifficultyAdjustmentWindowSize,
            min_difficulty_window_size: self.DifficultyAdjustmentWindowSize,
            prior_mergeset_size_limit: self.MergeSetSizeLimit,
            prior_merge_depth: self.MergeDepth,
            prior_finality_depth: finality_depth,
            prior_pruning_depth: 2 * finality_depth + 4 * self.MergeSetSizeLimit * self.K as u64 + 2 * self.K as u64 + 2,
            coinbase_payload_script_public_key_max_len: self.CoinbasePayloadScriptPublicKeyMaxLength,
            max_coinbase_payload_len: self.MaxCoinbasePayloadLength,
            prior_max_tx_inputs: MAINNET_PARAMS.prior_max_tx_inputs,
            prior_max_tx_outputs: MAINNET_PARAMS.prior_max_tx_outputs,
            prior_max_signature_script_len: MAINNET_PARAMS.prior_max_signature_script_len,
            prior_max_script_public_key_len: MAINNET_PARAMS.prior_max_script_public_key_len,
            mass_per_tx_byte: self.MassPerTxByte,
            mass_per_script_pub_key_byte: self.MassPerScriptPubKeyByte,
            mass_per_sig_op: self.MassPerSigOp,
            max_block_mass: self.MaxBlockMass,
            storage_mass_parameter: STORAGE_MASS_PARAMETER,
            deflationary_phase_daa_score: self.DeflationaryPhaseDaaScore,
            pre_deflationary_phase_base_subsidy: self.PreDeflationaryPhaseBaseSubsidy,
            prior_coinbase_maturity: MAINNET_PARAMS.prior_coinbase_maturity,
            skip_proof_of_work: self.SkipProofOfWork,
            max_block_level: self.MaxBlockLevel,
            pruning_proof_m: self.PruningProofM,
            crescendo: CRESCENDO,
            crescendo_activation: ForkActivation::never(),
        }
    }
}

#[tokio::test]
async fn goref_custom_pruning_depth_test() {
    init_allocator_with_default_settings();
    json_test("testdata/dags_for_json_tests/goref_custom_pruning_depth", false).await
}

#[tokio::test]
async fn goref_notx_test() {
    init_allocator_with_default_settings();
    json_test("testdata/dags_for_json_tests/goref-notx-5000-blocks", false).await
}

#[tokio::test]
async fn goref_notx_concurrent_test() {
    init_allocator_with_default_settings();
    json_test("testdata/dags_for_json_tests/goref-notx-5000-blocks", true).await
}

#[tokio::test]
async fn goref_tx_small_test() {
    init_allocator_with_default_settings();
    json_test("testdata/dags_for_json_tests/goref-905-tx-265-blocks", false).await
}

#[tokio::test]
async fn goref_tx_small_concurrent_test() {
    init_allocator_with_default_settings();
    json_test("testdata/dags_for_json_tests/goref-905-tx-265-blocks", true).await
}

#[ignore]
#[tokio::test]
async fn goref_tx_big_test() {
    init_allocator_with_default_settings();
    // TODO: add this directory to a data repo and fetch dynamically
    json_test("testdata/dags_for_json_tests/goref-1.6M-tx-10K-blocks", false).await
}

#[ignore]
#[tokio::test]
async fn goref_tx_big_concurrent_test() {
    init_allocator_with_default_settings();
    // TODO: add this file to a data repo and fetch dynamically
    json_test("testdata/dags_for_json_tests/goref-1.6M-tx-10K-blocks", true).await
}

#[tokio::test]
#[ignore = "long"]
async fn goref_mainnet_test() {
    // TODO: add this directory to a data repo and fetch dynamically
    json_test("testdata/dags_for_json_tests/goref-mainnet", false).await
}

#[tokio::test]
#[ignore = "long"]
async fn goref_mainnet_concurrent_test() {
    // TODO: add this directory to a data repo and fetch dynamically
    json_test("testdata/dags_for_json_tests/goref-mainnet", true).await
}

fn gzip_file_lines(path: &Path) -> impl Iterator<Item = String> {
    let file = common::open_file(path);
    let decoder = GzDecoder::new(file);
    BufReader::new(decoder).lines().map(|line| line.unwrap())
}

async fn json_test(file_path: &str, concurrency: bool) {
    kaspa_core::log::try_init_logger("info");
    let main_path = Path::new(file_path);
    let proof_exists = common::file_exists(&main_path.join("proof.json.gz"));

    let mut lines = gzip_file_lines(&main_path.join("blocks.json.gz"));
    let first_line = lines.next().unwrap();
    let go_params_res: Result<KaspadGoParams, _> = serde_json::from_str(&first_line);
    let params = if let Ok(go_params) = go_params_res {
        let mut params = go_params.into_params();
        if !proof_exists {
            let second_line = lines.next().unwrap();
            let genesis_block = json_line_to_block(second_line);
            params.genesis = (genesis_block.header.as_ref(), DEVNET_PARAMS.genesis.coinbase_payload).into();
        }
        params.min_difficulty_window_size = params.prior_difficulty_window_size;
        params
    } else {
        let genesis_block = json_line_to_block(first_line);
        let mut params = DEVNET_PARAMS;
        params.genesis = (genesis_block.header.as_ref(), params.genesis.coinbase_payload).into();
        params.min_difficulty_window_size = params.prior_difficulty_window_size;
        params
    };

    let mut config = Config::new(params);
    if proof_exists {
        config.process_genesis = false;
    }
    let config = Arc::new(config);

    let tick_service = Arc::new(TickService::default());
    let (notification_send, notification_recv) = unbounded();
    let subscription_context = SubscriptionContext::new();
    let tc = Arc::new(TestConsensus::with_notifier(&config, notification_send, subscription_context.clone()));
    let notify_service = Arc::new(NotifyService::new(tc.notification_root(), notification_recv, subscription_context.clone()));

    // External storage for storing block bodies. This allows separating header and body processing phases
    let (_external_db_lifetime, external_storage) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
    let external_block_store = DbBlockTransactionsStore::new(external_storage, CachePolicy::Count(config.perf.block_data_cache_size));
    let (_utxoindex_db_lifetime, utxoindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).unwrap();
    let consensus_manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(tc.clone()))));
    let utxoindex = UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap();
    let index_service = Arc::new(IndexService::new(
        &notify_service.notifier(),
        subscription_context.clone(),
        Some(UtxoIndexProxy::new(utxoindex.clone())),
    ));

    let async_runtime = Arc::new(AsyncRuntime::new(2));
    async_runtime.register(tick_service.clone());
    async_runtime.register(notify_service.clone());
    async_runtime.register(index_service.clone());
    async_runtime.register(Arc::new(ConsensusMonitor::new(tc.processing_counters().clone(), tick_service)));

    let core = Arc::new(Core::new());
    core.bind(consensus_manager);
    core.bind(async_runtime);
    let joins = core.start();

    let pruning_point = if proof_exists {
        let proof_lines = gzip_file_lines(&main_path.join("proof.json.gz"));
        let proof = proof_lines
            .map(|line| {
                let rpc_headers: Vec<RPCBlockHeader> = serde_json::from_str(&line).unwrap();
                rpc_headers.iter().map(|rh| Arc::new(rpc_header_to_header(rh))).collect_vec()
            })
            .collect_vec();

        let trusted_blocks = gzip_file_lines(&main_path.join("trusted.json.gz")).map(json_trusted_line_to_block_and_gd).collect_vec();
        tc.apply_pruning_proof(proof, &trusted_blocks).unwrap();

        let past_pruning_points =
            gzip_file_lines(&main_path.join("past-pps.json.gz")).map(|line| json_line_to_block(line).header).collect_vec();
        let pruning_point = past_pruning_points.last().unwrap().hash;

        tc.import_pruning_points(past_pruning_points).unwrap();

        info!("Processing {} trusted blocks...", trusted_blocks.len());
        for tb in trusted_blocks.into_iter() {
            tc.validate_and_insert_trusted_block(tb).virtual_state_task.await.unwrap();
        }
        Some(pruning_point)
    } else {
        None
    };

    info!("Processing block headers...");

    if concurrency {
        let chunks = lines.chunks(1000);
        let mut iter = chunks.into_iter();
        let chunk = iter.next().unwrap();
        let mut prev_joins = submit_header_chunk(&tc, &external_block_store, chunk);

        for chunk in iter {
            let current_joins = submit_header_chunk(&tc, &external_block_store, chunk);
            let statuses = try_join_all(prev_joins).await.unwrap();
            assert!(statuses.iter().all(|s| s.is_header_only()));
            prev_joins = current_joins;
        }

        let statuses = try_join_all(prev_joins).await.unwrap();
        assert!(statuses.iter().all(|s| s.is_header_only()));
    } else {
        for line in lines {
            let block = json_line_to_block(line);
            let hash = block.header.hash;
            // Test our hashing implementation vs the hash accepted from the json source
            assert_eq!(hashing::header::hash(&block.header), hash, "header hashing for block {hash} failed");

            external_block_store.insert(hash, block.transactions).unwrap();
            let block = Block::from_header_arc(block.header);
            let status =
                tc.validate_and_insert_block(block).virtual_state_task.await.unwrap_or_else(|e| panic!("block {hash} failed: {e}"));
            assert!(status.is_header_only());
        }
    }

    if proof_exists {
        info!("Importing the UTXO set...");
        let mut multiset = MuHash::new();
        for outpoint_utxo_pairs in gzip_file_lines(&main_path.join("pp-utxo.json.gz")).map(json_line_to_utxo_pairs) {
            tc.append_imported_pruning_point_utxos(&outpoint_utxo_pairs, &mut multiset);
        }

        tc.import_pruning_point_utxo_set(pruning_point.unwrap(), multiset).unwrap();
        utxoindex.write().resync().unwrap();
        // TODO: Add consensus validation that the pruning point is actually the right block according to the rules (in pruning depth etc).
    }

    let missing_bodies = tc.get_missing_block_body_hashes(tc.get_headers_selected_tip()).unwrap();

    info!("Processing {} block bodies...", missing_bodies.len());

    if concurrency {
        let chunks = missing_bodies.into_iter().chunks(1000);
        let mut iter = chunks.into_iter();
        let chunk = iter.next().unwrap();
        let mut prev_joins = submit_body_chunk(&tc, &external_block_store, chunk);

        for chunk in iter {
            let current_joins = submit_body_chunk(&tc, &external_block_store, chunk);
            let statuses = try_join_all(prev_joins).await.unwrap();
            assert!(statuses.iter().all(|s| s.is_utxo_valid_or_pending()));
            prev_joins = current_joins;
        }

        let statuses = try_join_all(prev_joins).await.unwrap();
        assert!(statuses.iter().all(|s| s.is_utxo_valid_or_pending()));
    } else {
        for hash in missing_bodies {
            let block = Block::from_arcs(tc.get_header(hash).unwrap(), external_block_store.get(hash).unwrap());
            let status =
                tc.validate_and_insert_block(block).virtual_state_task.await.unwrap_or_else(|e| panic!("block {hash} failed: {e}"));
            assert!(status.is_utxo_valid_or_pending());
        }
    }

    core.shutdown();
    core.join(joins);

    // Assert that at least one body tip was resolved with valid UTXO
    assert!(tc.body_tips().iter().copied().any(|h| tc.block_status(h) == BlockStatus::StatusUTXOValid));
    // Assert that the indexed selected chain store matches the virtual chain obtained
    // through the reachability iterator
    assert_selected_chain_store_matches_virtual_chain(&tc);
    let virtual_utxos: HashSet<TransactionOutpoint> =
        HashSet::from_iter(tc.get_virtual_utxos(None, usize::MAX, false).into_iter().map(|(outpoint, _)| outpoint));
    let utxoindex_utxos = utxoindex.read().get_all_outpoints().unwrap();
    assert_eq!(virtual_utxos.len(), utxoindex_utxos.len());
    assert!(virtual_utxos.is_subset(&utxoindex_utxos));
    assert!(utxoindex_utxos.is_subset(&virtual_utxos));
}

fn submit_header_chunk(
    tc: &TestConsensus,
    external_block_store: &DbBlockTransactionsStore,
    chunk: impl Iterator<Item = String>,
) -> Vec<impl Future<Output = BlockProcessResult<BlockStatus>>> {
    let mut futures = Vec::new();
    for line in chunk {
        let block = json_line_to_block(line);
        external_block_store.insert(block.hash(), block.transactions).unwrap();
        let block = Block::from_header_arc(block.header);
        let f = tc.validate_and_insert_block(block).virtual_state_task;
        futures.push(f);
    }
    futures
}

fn submit_body_chunk(
    tc: &TestConsensus,
    external_block_store: &DbBlockTransactionsStore,
    chunk: impl Iterator<Item = Hash>,
) -> Vec<impl Future<Output = BlockProcessResult<BlockStatus>>> {
    let mut futures = Vec::new();
    for hash in chunk {
        let block = Block::from_arcs(tc.get_header(hash).unwrap(), external_block_store.get(hash).unwrap());
        let f = tc.validate_and_insert_block(block).virtual_state_task;
        futures.push(f);
    }
    futures
}

fn rpc_header_to_header(rpc_header: &RPCBlockHeader) -> Header {
    Header::new_finalized(
        rpc_header.Version,
        rpc_header
            .Parents
            .iter()
            .map(|item| item.ParentHashes.iter().map(|parent| Hash::from_str(parent).unwrap()).collect())
            .collect(),
        Hash::from_str(&rpc_header.HashMerkleRoot).unwrap(),
        Hash::from_str(&rpc_header.AcceptedIDMerkleRoot).unwrap(),
        Hash::from_str(&rpc_header.UTXOCommitment).unwrap(),
        rpc_header.Timestamp,
        rpc_header.Bits,
        rpc_header.Nonce,
        rpc_header.DAAScore,
        BlueWorkType::from_hex(&rpc_header.BlueWork).unwrap(),
        rpc_header.BlueScore,
        Hash::from_str(&rpc_header.PruningPoint).unwrap(),
    )
}

fn json_trusted_line_to_block_and_gd(line: String) -> TrustedBlock {
    let json_block_with_trusted: JsonBlockWithTrustedData = serde_json::from_str(&line).unwrap();
    let block = rpc_block_to_block(json_block_with_trusted.Block);

    let gd = ExternalGhostdagData {
        blue_score: json_block_with_trusted.GHOSTDAG.BlueScore,
        blue_work: BlueWorkType::from_hex(&json_block_with_trusted.GHOSTDAG.BlueWork).unwrap(),
        selected_parent: Hash::from_str(&json_block_with_trusted.GHOSTDAG.SelectedParent).unwrap(),
        mergeset_blues: json_block_with_trusted
            .GHOSTDAG
            .MergeSetBlues
            .into_iter()
            .map(|hex| Hash::from_str(&hex).unwrap())
            .collect_vec(),

        mergeset_reds: json_block_with_trusted
            .GHOSTDAG
            .MergeSetReds
            .into_iter()
            .map(|hex| Hash::from_str(&hex).unwrap())
            .collect_vec(),

        blues_anticone_sizes: BlockHashMap::from_iter(
            json_block_with_trusted
                .GHOSTDAG
                .BluesAnticoneSizes
                .into_iter()
                .map(|e| (Hash::from_str(&e.BlueHash).unwrap(), e.AnticoneSize)),
        ),
    };

    TrustedBlock::new(block, gd)
}

fn json_line_to_utxo_pairs(line: String) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let json_pairs: Vec<JsonOutpointUTXOEntryPair> = serde_json::from_str(&line).unwrap();
    json_pairs
        .iter()
        .map(|json_pair| {
            (
                TransactionOutpoint {
                    transaction_id: Hash::from_str(&json_pair.Outpoint.TransactionID).unwrap(),
                    index: json_pair.Outpoint.Index,
                },
                UtxoEntry {
                    amount: json_pair.UTXOEntry.Amount,
                    script_public_key: ScriptPublicKey::from_vec(
                        json_pair.UTXOEntry.ScriptPublicKey.Version,
                        hex_decode(&json_pair.UTXOEntry.ScriptPublicKey.Script),
                    ),
                    block_daa_score: json_pair.UTXOEntry.BlockDAAScore,
                    is_coinbase: json_pair.UTXOEntry.IsCoinbase,
                },
            )
        })
        .collect_vec()
}

fn json_line_to_block(line: String) -> Block {
    let rpc_block: RPCBlock = serde_json::from_str(&line).unwrap();
    rpc_block_to_block(rpc_block)
}

fn rpc_block_to_block(rpc_block: RPCBlock) -> Block {
    let header = rpc_header_to_header(&rpc_block.Header);
    assert_eq!(header.hash, Hash::from_str(&rpc_block.VerboseData.Hash).unwrap());
    Block::new(
        header,
        rpc_block
            .Transactions
            .iter()
            .map(|tx| {
                Transaction::new(
                    tx.Version,
                    tx.Inputs
                        .iter()
                        .map(|input| TransactionInput {
                            previous_outpoint: TransactionOutpoint {
                                transaction_id: Hash::from_str(&input.PreviousOutpoint.TransactionID).unwrap(),
                                index: input.PreviousOutpoint.Index,
                            },
                            signature_script: hex_decode(&input.SignatureScript),
                            sequence: input.Sequence,
                            sig_op_count: input.SigOpCount,
                        })
                        .collect(),
                    tx.Outputs
                        .iter()
                        .map(|output| TransactionOutput {
                            value: output.Amount,
                            script_public_key: ScriptPublicKey::from_vec(
                                output.ScriptPublicKey.Version,
                                hex_decode(&output.ScriptPublicKey.Script),
                            ),
                        })
                        .collect(),
                    tx.LockTime,
                    SubnetworkId::from_str(&tx.SubnetworkID).unwrap(),
                    tx.Gas,
                    hex_decode(&tx.Payload),
                )
            })
            .collect(),
    )
}

fn hex_decode(src: &str) -> Vec<u8> {
    if src.is_empty() {
        return Vec::new();
    }
    let mut dst: Vec<u8> = vec![0; src.len() / 2];
    faster_hex::hex_decode(src.as_bytes(), &mut dst).unwrap();
    dst
}

#[tokio::test]
async fn bounded_merge_depth_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(DEVNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.prior_ghostdag_k = 5;
            p.prior_merge_depth = 7;
        })
        .build();

    assert!(
        (config.ghostdag_k().before() as u64) < config.prior_merge_depth,
        "K must be smaller than merge depth for this test to run"
    );

    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let mut selected_chain = vec![config.genesis.hash];
    for i in 1..(config.prior_merge_depth + 3) {
        let hash: Hash = (i + 1).into();
        consensus.add_block_with_parents(hash, vec![*selected_chain.last().unwrap()]).await.unwrap();
        selected_chain.push(hash);
    }

    // The length of block_chain_2 is shorter by one than selected_chain, so selected_chain will remain the selected chain.
    let mut block_chain_2 = vec![config.genesis.hash];
    for i in 1..(config.prior_merge_depth + 2) {
        let hash: Hash = (i + config.prior_merge_depth + 3).into();
        consensus.add_block_with_parents(hash, vec![*block_chain_2.last().unwrap()]).await.unwrap();
        block_chain_2.push(hash);
    }

    // The merge depth root belongs to selected_chain, and block_chain_2[1] is red and doesn't have it in its past, and is not in the
    // past of any kosherizing block, so we expect the next block to be rejected.
    match consensus.add_block_with_parents(100.into(), vec![block_chain_2[1], *selected_chain.last().unwrap()]).await {
        Err(RuleError::ViolatingBoundedMergeDepth) => {}
        res => panic!("Unexpected result: {res:?}"),
    }

    // A block that points to tip of both chains will be rejected for similar reasons (since block_chain_2 tip is also red).
    match consensus.add_block_with_parents(101.into(), vec![*block_chain_2.last().unwrap(), *selected_chain.last().unwrap()]).await {
        Err(RuleError::ViolatingBoundedMergeDepth) => {}
        res => panic!("Unexpected result: {res:?}"),
    }

    let kosherizing_hash: Hash = 102.into();
    // This will pass since now genesis is the mutual merge depth root.
    consensus
        .add_block_with_parents(
            kosherizing_hash,
            vec![block_chain_2[block_chain_2.len() - 3], selected_chain[selected_chain.len() - 3]],
        )
        .await
        .unwrap();

    let point_at_blue_kosherizing: Hash = 103.into();
    // We expect it to pass because all of the reds are in the past of a blue kosherizing block.
    consensus
        .add_block_with_parents(point_at_blue_kosherizing, vec![kosherizing_hash, *selected_chain.last().unwrap()])
        .await
        .unwrap();

    // We extend the selected chain until kosherizing_hash will be red from the virtual POV.
    for i in 0..config.ghostdag_k().before() {
        let hash = Hash::from_u64_word((i + 1) as u64 * 1000);
        consensus.add_block_with_parents(hash, vec![*selected_chain.last().unwrap()]).await.unwrap();
        selected_chain.push(hash);
    }

    // Since kosherizing_hash is now red, we expect this to fail.
    match consensus.add_block_with_parents(1200.into(), vec![kosherizing_hash, *selected_chain.last().unwrap()]).await {
        Err(RuleError::ViolatingBoundedMergeDepth) => {}
        res => panic!("Unexpected result: {res:?}"),
    }

    // point_at_blue_kosherizing is kosherizing kosherizing_hash, so this should pass.
    consensus.add_block_with_parents(1201.into(), vec![point_at_blue_kosherizing, *selected_chain.last().unwrap()]).await.unwrap();

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn difficulty_test() {
    init_allocator_with_default_settings();
    async fn add_block(consensus: &TestConsensus, block_time: Option<u64>, parents: Vec<Hash>) -> Header {
        let selected_parent = consensus.ghostdag_manager().find_selected_parent(parents.iter().copied());
        let block_time = block_time.unwrap_or_else(|| {
            consensus.headers_store().get_timestamp(selected_parent).unwrap() + consensus.params().prior_target_time_per_block
        });
        let mut header = consensus.build_header_with_parents(new_unique(), parents);
        header.timestamp = block_time;
        consensus.validate_and_insert_block(Block::new(header.clone(), vec![])).virtual_state_task.await.unwrap();
        header
    }

    fn past_median_time(consensus: &TestConsensus, parents: &[Hash]) -> u64 {
        let ghostdag_data = consensus.ghostdag_manager().ghostdag(parents);
        consensus.window_manager().calc_past_median_time(&ghostdag_data).unwrap().0
    }

    async fn add_block_with_min_time(consensus: &TestConsensus, parents: Vec<Hash>) -> Header {
        let pmt = past_median_time(consensus, &parents[..]);
        add_block(consensus, Some(pmt + 1), parents).await
    }

    fn compare_bits(a: u32, b: u32) -> Ordering {
        Uint256::from_compact_target_bits(a).cmp(&Uint256::from_compact_target_bits(b))
    }

    fn full_window_bits(consensus: &TestConsensus, hash: Hash) -> u32 {
        let window_size =
            consensus.params().difficulty_window_size().before() * consensus.params().difficulty_sample_rate().before() as usize;
        let ghostdag_data = &consensus.ghostdag_store().get_data(hash).unwrap();
        let window = consensus.window_manager().block_window(ghostdag_data, WindowType::VaryingWindow(window_size)).unwrap();
        assert_eq!(window.blocks.len(), window_size);
        let daa_window = consensus.window_manager().calc_daa_window(ghostdag_data, window);
        consensus.window_manager().calculate_difficulty_bits(ghostdag_data, &daa_window)
    }

    struct Test {
        name: &'static str,
        enabled: bool,
        config: Config,
    }

    const FULL_WINDOW_SIZE: usize = 90;
    const SAMPLED_WINDOW_SIZE: u64 = 11;
    const SAMPLE_RATE: u64 = 6;
    const PMT_DEVIATION_TOLERANCE: u64 = 20;
    const PMT_SAMPLE_RATE: u64 = 3;
    const PMT_SAMPLED_WINDOW_SIZE: u64 = 13;
    const HIGH_BPS_SAMPLED_WINDOW_SIZE: u64 = 12;
    const HIGH_BPS: u64 = 4;
    let tests = vec![
        Test {
            name: "MAINNET with full window",
            enabled: true,
            config: ConfigBuilder::new(MAINNET_PARAMS)
                .skip_proof_of_work()
                .edit_consensus_params(|p| {
                    p.prior_ghostdag_k = 1;
                    p.prior_difficulty_window_size = FULL_WINDOW_SIZE;
                    p.crescendo_activation = ForkActivation::never();
                    // Define past median time so that calls to add_block_with_min_time create blocks
                    // which timestamps fit within the min-max timestamps found in the difficulty window
                    p.timestamp_deviation_tolerance = 60;
                })
                .build(),
        },
        Test {
            name: "MAINNET with sampled window",
            enabled: true,
            config: ConfigBuilder::new(MAINNET_PARAMS)
                .skip_proof_of_work()
                .edit_consensus_params(|p| {
                    p.prior_ghostdag_k = 1;
                    p.crescendo.ghostdag_k = 1;
                    p.crescendo.sampled_difficulty_window_size = SAMPLED_WINDOW_SIZE;
                    p.crescendo.difficulty_sample_rate = SAMPLE_RATE;
                    p.crescendo_activation = ForkActivation::always();
                    p.prior_target_time_per_block = p.crescendo.target_time_per_block;
                    // Define past median time so that calls to add_block_with_min_time create blocks
                    // which timestamps fit within the min-max timestamps found in the difficulty window
                    p.crescendo.past_median_time_sample_rate = PMT_SAMPLE_RATE;
                    p.crescendo.past_median_time_sampled_window_size = PMT_SAMPLED_WINDOW_SIZE;
                    p.timestamp_deviation_tolerance = PMT_DEVIATION_TOLERANCE;
                })
                .build(),
        },
        Test {
            name: "MAINNET with sampled window & high BPS",
            enabled: false,
            config: ConfigBuilder::new(MAINNET_PARAMS)
                .skip_proof_of_work()
                .edit_consensus_params(|p| {
                    p.prior_ghostdag_k = 1;
                    p.crescendo.ghostdag_k = 1;
                    p.prior_target_time_per_block /= HIGH_BPS;
                    p.crescendo.sampled_difficulty_window_size = HIGH_BPS_SAMPLED_WINDOW_SIZE;
                    p.crescendo.difficulty_sample_rate = SAMPLE_RATE * HIGH_BPS;
                    p.crescendo_activation = ForkActivation::always();
                    p.prior_target_time_per_block = p.crescendo.target_time_per_block;
                    // Define past median time so that calls to add_block_with_min_time create blocks
                    // which timestamps fit within the min-max timestamps found in the difficulty window
                    p.crescendo.past_median_time_sample_rate = PMT_SAMPLE_RATE * HIGH_BPS;
                    p.crescendo.past_median_time_sampled_window_size = PMT_SAMPLED_WINDOW_SIZE;
                    p.timestamp_deviation_tolerance = PMT_DEVIATION_TOLERANCE;
                })
                .build(),
        },
    ];

    kaspa_core::log::try_init_logger("info");
    for test in tests.iter().filter(|x| x.enabled) {
        let consensus = TestConsensus::new(&test.config);
        let wait_handles = consensus.init();

        let sample_rate = test.config.difficulty_sample_rate().before();
        let expanded_window_size = test.config.difficulty_window_size().before() * sample_rate as usize;

        let fake_genesis = Header {
            hash: test.config.genesis.hash,
            version: 0,
            parents_by_level: vec![],
            hash_merkle_root: 0.into(),
            accepted_id_merkle_root: 0.into(),
            utxo_commitment: 0.into(),
            timestamp: 0,
            bits: 0,
            nonce: 0,
            daa_score: 0,
            blue_work: 0.into(),
            blue_score: 0,
            pruning_point: 0.into(),
        };

        // Stage 0
        info!("{} - Stage 0", test.name);
        let mut tip = fake_genesis;
        for i in 0..expanded_window_size {
            tip = add_block(&consensus, None, vec![tip.hash]).await;
            assert_eq!(
                tip.bits, test.config.genesis.bits,
                "{}: {} until first DAA window is created difficulty should remain unchanged",
                test.name, i
            );
        }

        // Stage 1
        info!("{} - Stage 1", test.name);
        for _ in 0..expanded_window_size + 10 {
            tip = add_block(&consensus, None, vec![tip.hash]).await;
            assert_eq!(
                tip.bits, test.config.genesis.bits,
                "{}: block rate wasn't changed so difficulty is not expected to change",
                test.name
            );
        }
        let stage_1_bits = full_window_bits(&consensus, tip.hash);

        // Stage 2
        // Add exactly one block in the past to the window
        info!("{} - Stage 2", test.name);
        for _ in 0..sample_rate {
            if (tip.daa_score + 1) % sample_rate == 0 {
                // This block should be part of the sampled window
                let block_in_the_past = add_block_with_min_time(&consensus, vec![tip.hash]).await;
                tip = block_in_the_past;
                break;
            } else {
                tip = add_block(&consensus, None, vec![tip.hash]).await;
            }
        }
        [(tip.bits, test.config.genesis.bits), (full_window_bits(&consensus, tip.hash), stage_1_bits)].iter().for_each(|(a, b)| {
            assert_eq!(*a, *b, "{}: block_in_the_past shouldn't affect its own difficulty, but only its future", test.name);
        });
        for _ in 0..sample_rate {
            tip = add_block(&consensus, None, vec![tip.hash]).await;
        }
        let stage_2_bits = full_window_bits(&consensus, tip.hash);
        [(tip.bits, test.config.genesis.bits), (stage_2_bits, stage_1_bits)].iter().for_each(|(a, b)| {
            assert_eq!(
                compare_bits(*a, *b),
                Ordering::Less,
                "{}: block_in_the_past should affect the difficulty of its future",
                test.name
            );
        });
        let one_block_in_the_past_bits = tip.bits;

        // Stage 3
        // Increase block rate to increase difficulty
        info!("{} - Stage 3", test.name);
        for _ in 0..expanded_window_size {
            let prev_bits = tip.bits;
            tip = add_block_with_min_time(&consensus, vec![tip.hash]).await;
            assert!(
                compare_bits(tip.bits, prev_bits) != Ordering::Greater,
                "{}: because we're increasing the block rate, the difficulty can't decrease",
                test.name
            );
        }
        let stage_3_bits = full_window_bits(&consensus, tip.hash);
        [(tip.bits, one_block_in_the_past_bits), (stage_3_bits, stage_2_bits)].iter().for_each(|(a, b)| {
            assert_eq!(
                compare_bits(*a, *b),
                Ordering::Less,
                "{}: since we increased the block rate in the whole window, we expect the difficulty to be increased",
                test.name
            );
        });

        // Stage 4
        // Add blocks until difficulty stabilizes
        info!("{} - Stage 4", test.name);
        let mut same_bits_count = 0;
        while same_bits_count < expanded_window_size + 1 {
            let prev_bits = tip.bits;
            tip = add_block(&consensus, None, vec![tip.hash]).await;
            if tip.bits == prev_bits {
                same_bits_count += 1;
            } else {
                same_bits_count = 0;
            }
        }
        let stage_4_bits = full_window_bits(&consensus, tip.hash);

        // Stage 5
        // Add a slow block
        info!("{} - Stage 5", test.name);
        let pre_slow_block_bits = tip.bits;
        for _ in 0..sample_rate {
            if (tip.daa_score + 1) % sample_rate == 0 {
                // This block should be part of the sampled window
                let slow_block_time = tip.timestamp + test.config.prior_target_time_per_block * 3;
                let slow_block = add_block(&consensus, Some(slow_block_time), vec![tip.hash]).await;
                tip = slow_block;
                break;
            } else {
                tip = add_block(&consensus, None, vec![tip.hash]).await;
            }
        }
        [(tip.bits, pre_slow_block_bits), (full_window_bits(&consensus, tip.hash), stage_4_bits)].iter().for_each(|(a, b)| {
            assert_eq!(*a, *b, "{}: the difficulty should change only when slow_block is in the past", test.name);
        });

        for _ in 0..sample_rate {
            tip = add_block(&consensus, None, vec![tip.hash]).await;
        }
        let stage_5_bits = full_window_bits(&consensus, tip.hash);
        [(tip.bits, pre_slow_block_bits), (stage_5_bits, stage_4_bits)].iter().for_each(|(a, b)| {
            assert_eq!(
                compare_bits(*a, *b),
                Ordering::Greater,
                "{}: block rate was decreased due to slow_block, so we expected difficulty to be reduced",
                test.name
            );
        });

        // Stage 6
        // Here we create two chains: a chain of blue blocks, and a chain of red blocks with
        // very low timestamps. Because the red blocks should be part of the difficulty
        // window, their low timestamps should lower the difficulty, and we check it by
        // comparing the bits of two blocks with the same blue score, one with the red
        // blocks in its past and one without.
        info!("{} - Stage 6", test.name);
        let split_hash = tip.hash;
        let mut blue_tip_hash = split_hash;
        for _ in 0..expanded_window_size {
            blue_tip_hash = add_block(&consensus, None, vec![blue_tip_hash]).await.hash;
        }

        let split_hash = tip.hash;
        let mut red_tip_hash = split_hash;
        let red_chain_len = max(sample_rate as usize * 2, 10);
        for _ in 0..red_chain_len {
            red_tip_hash = add_block(&consensus, None, vec![red_tip_hash]).await.hash;
        }

        let tip_with_red_past = add_block(&consensus, None, vec![red_tip_hash, blue_tip_hash]).await;
        let tip_without_red_past = add_block(&consensus, None, vec![blue_tip_hash]).await;
        [
            (tip_with_red_past.bits, tip_without_red_past.bits),
            (full_window_bits(&consensus, tip_with_red_past.hash), full_window_bits(&consensus, tip_without_red_past.hash)),
        ]
        .iter()
        .for_each(|(a, b)| {
            assert_eq!(
                compare_bits(*a, *b),
                Ordering::Less,
                "{}: we expect the red blocks to increase the difficulty of tip_with_red_past",
                test.name
            );
        });

        // Stage 7
        // We repeat the test, but now we make the blue chain longer in order to filter
        // out the red blocks from the window, and check that the red blocks don't
        // affect the difficulty.
        info!("{} - Stage 7", test.name);
        blue_tip_hash = split_hash;
        for _ in 0..expanded_window_size + red_chain_len + sample_rate as usize {
            blue_tip_hash = add_block(&consensus, None, vec![blue_tip_hash]).await.hash;
        }

        red_tip_hash = split_hash;
        for _ in 0..red_chain_len {
            red_tip_hash = add_block(&consensus, None, vec![red_tip_hash]).await.hash;
        }

        let tip_with_red_past = add_block(&consensus, None, vec![red_tip_hash, blue_tip_hash]).await;
        let tip_without_red_past = add_block(&consensus, None, vec![blue_tip_hash]).await;
        [
            (tip_with_red_past.bits, tip_without_red_past.bits),
            (full_window_bits(&consensus, tip_with_red_past.hash), full_window_bits(&consensus, tip_without_red_past.hash)),
        ]
        .iter()
        .for_each(|(a, b)| {
            assert_eq!(*a, *b, "{}: we expect the red blocks to not affect the difficulty of tip_with_red_past", test.name);
        });

        consensus.shutdown(wait_handles);
    }
}

#[tokio::test]
async fn selected_chain_test() {
    init_allocator_with_default_settings();
    kaspa_core::log::try_init_logger("info");

    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.min_difficulty_window_size = p.prior_difficulty_window_size;
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    consensus.add_utxo_valid_block_with_parents(1.into(), vec![config.genesis.hash], vec![]).await.unwrap();
    for i in 2..7 {
        let hash = i.into();
        consensus.add_utxo_valid_block_with_parents(hash, vec![(i - 1).into()], vec![]).await.unwrap();
    }
    consensus.add_utxo_valid_block_with_parents(7.into(), vec![1.into()], vec![]).await.unwrap(); // Adding a non chain block shouldn't affect the selected chain store.

    assert_eq!(consensus.selected_chain_store.read().get_by_index(0).unwrap(), config.genesis.hash);
    for i in 1..7 {
        assert_eq!(consensus.selected_chain_store.read().get_by_index(i).unwrap(), i.into());
    }
    assert!(consensus.selected_chain_store.read().get_by_index(7).is_err());

    consensus.add_utxo_valid_block_with_parents(8.into(), vec![config.genesis.hash], vec![]).await.unwrap();
    for i in 9..15 {
        let hash = i.into();
        consensus.add_utxo_valid_block_with_parents(hash, vec![(i - 1).into()], vec![]).await.unwrap();
    }

    assert_eq!(consensus.selected_chain_store.read().get_by_index(0).unwrap(), config.genesis.hash);
    for i in 1..8 {
        assert_eq!(consensus.selected_chain_store.read().get_by_index(i).unwrap(), (i + 7).into());
    }
    assert!(consensus.selected_chain_store.read().get_by_index(8).is_err());

    // We now check a situation where there's a shorter selected chain (3 blocks) with more blue work
    for i in 15..23 {
        consensus.add_utxo_valid_block_with_parents(i.into(), vec![config.genesis.hash], vec![]).await.unwrap();
    }
    consensus.add_utxo_valid_block_with_parents(23.into(), (15..23).map(|i| i.into()).collect_vec(), vec![]).await.unwrap();

    assert_eq!(consensus.selected_chain_store.read().get_by_index(0).unwrap(), config.genesis.hash);
    assert_eq!(consensus.selected_chain_store.read().get_by_index(1).unwrap(), 22.into()); // We expect 23's selected parent to be 22 because of GHOSTDAG tie-breaking rules.
    assert_eq!(consensus.selected_chain_store.read().get_by_index(2).unwrap(), 23.into());
    assert!(consensus.selected_chain_store.read().get_by_index(3).is_err());
    assert_selected_chain_store_matches_virtual_chain(&consensus);

    consensus.shutdown(wait_handles);
}

fn assert_selected_chain_store_matches_virtual_chain(consensus: &TestConsensus) {
    let pruning_point = consensus.pruning_point();
    let iter1 = selected_chain_store_iterator(consensus, pruning_point);
    let iter2 = consensus.reachability_service().backward_chain_iterator(consensus.get_sink(), pruning_point, false);
    itertools::assert_equal(iter1, iter2);
}

fn selected_chain_store_iterator(consensus: &TestConsensus, pruning_point: Hash) -> impl Iterator<Item = Hash> + '_ {
    let selected_chain_read = consensus.selected_chain_store.read();
    let (idx, current) = selected_chain_read.get_tip().unwrap();
    std::iter::once(current)
        .chain((0..idx).rev().map(move |i| selected_chain_read.get_by_index(i).unwrap()))
        .take_while(move |&h| h != pruning_point)
}

#[tokio::test]
async fn staging_consensus_test() {
    init_allocator_with_default_settings();
    let config = ConfigBuilder::new(MAINNET_PARAMS).build();

    let db_tempdir = get_kaspa_tempdir().unwrap();
    let db_path = db_tempdir.path().to_owned();
    let consensus_db_dir = db_path.join("consensus");
    let meta_db_dir = db_path.join("meta");

    let meta_db = kaspa_database::prelude::ConnBuilder::default().with_db_path(meta_db_dir).with_files_limit(5).build().unwrap();

    let (notification_send, _notification_recv) = unbounded();
    let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_send));
    let counters = Arc::new(ProcessingCounters::default());
    let tx_script_cache_counters = Arc::new(TxScriptCacheCounters::default());

    let consensus_factory = Arc::new(ConsensusFactory::new(
        meta_db,
        &config,
        consensus_db_dir,
        4,
        notification_root,
        counters,
        tx_script_cache_counters,
        200,
        Arc::new(MiningRules::default()),
    ));
    let consensus_manager = Arc::new(ConsensusManager::new(consensus_factory));

    let core = Arc::new(Core::new());
    core.bind(consensus_manager.clone());
    let joins = core.start();

    let staging = consensus_manager.new_staging_consensus();
    staging.commit();

    core.shutdown();
    core.join(joins);
    drop(consensus_manager);
}

/// Tests the KIP-10 transaction introspection opcode activation by verifying that:
/// 1. Transactions using these opcodes are rejected before the activation DAA score
/// 2. The same transactions are accepted at and after the activation score
/// Uses OpInputSpk opcode as an example
#[tokio::test]
async fn run_kip10_activation_test() {
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
    use kaspa_txscript::opcodes::codes::{Op0, OpTxInputSpk};
    use kaspa_txscript::pay_to_script_hash_script;
    use kaspa_txscript::script_builder::ScriptBuilder;

    // KIP-10 activates at DAA score 3 in this test
    const KIP10_ACTIVATION_DAA_SCORE: u64 = 3;

    init_allocator_with_default_settings();

    // Create P2SH script that attempts to use OpInputSpk - this will be our test subject
    // The script should fail before KIP-10 activation and succeed after
    let redeem_script = ScriptBuilder::new()
        .add_op(Op0).unwrap() // Push 0 for input index
        .add_op(OpTxInputSpk).unwrap() // Get the input's script pubkey
        .drain();
    let spk = pay_to_script_hash_script(&redeem_script);

    // Set up initial UTXO with our test script
    let initial_utxo_collection = [(
        TransactionOutpoint::new(1.into(), 0),
        UtxoEntry { amount: SOMPI_PER_KASPA, script_public_key: spk.clone(), block_daa_score: 0, is_coinbase: false },
    )];

    // Initialize consensus with KIP-10 activation point
    let config = ConfigBuilder::new(DEVNET_PARAMS)
        .skip_proof_of_work()
        .apply_args(|cfg| {
            let mut genesis_multiset = MuHash::new();
            initial_utxo_collection.iter().for_each(|(outpoint, utxo)| {
                genesis_multiset.add_utxo(outpoint, utxo);
            });
            cfg.params.genesis.utxo_commitment = genesis_multiset.finalize();
            let genesis_header: Header = (&cfg.params.genesis).into();
            cfg.params.genesis.hash = genesis_header.hash;
        })
        .edit_consensus_params(|p| {
            p.crescendo_activation = ForkActivation::new(KIP10_ACTIVATION_DAA_SCORE);
        })
        .build();

    let consensus = TestConsensus::new(&config);
    let mut genesis_multiset = MuHash::new();
    consensus.append_imported_pruning_point_utxos(&initial_utxo_collection, &mut genesis_multiset);
    consensus.import_pruning_point_utxo_set(config.genesis.hash, genesis_multiset).unwrap();
    let wait_handles = consensus.init();

    // Build blockchain up to one block before activation
    let mut index = 0;
    for _ in 0..KIP10_ACTIVATION_DAA_SCORE - 1 {
        let parent = if index == 0 { config.genesis.hash } else { index.into() };
        consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![parent], vec![]).await.unwrap();
        index += 1;
    }
    assert_eq!(consensus.get_virtual_daa_score(), index);

    // Create transaction that attempts to use the KIP-10 opcode
    let mut tx = Transaction::new(
        0,
        vec![TransactionInput::new(
            initial_utxo_collection[0].0,
            ScriptBuilder::new().add_data(&redeem_script).unwrap().drain(),
            0,
            0,
        )],
        vec![TransactionOutput::new(initial_utxo_collection[0].1.amount - 5000, spk)],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    tx.finalize();
    let tx_id = tx.id();

    let mut tx = MutableTransaction::from_tx(tx);
    // This triggers storage mass population
    let _ = consensus.validate_mempool_transaction(&mut tx, &TransactionValidationArgs::default());
    let tx = tx.tx.unwrap_or_clone();

    // Test 1: Build empty block, then manually insert invalid tx and verify consensus rejects it
    {
        let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

        // First build block without transactions
        let mut block =
            consensus.build_utxo_valid_block_with_parents((index + 1).into(), vec![index.into()], miner_data.clone(), vec![]);

        // Insert our test transaction and recalculate block hashes
        block.transactions.push(tx.clone());
        block.header.hash_merkle_root =
            calc_hash_merkle_root(block.transactions.iter(), config.crescendo_activation.is_active(block.header.daa_score));
        let block_status = consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await;
        assert!(matches!(block_status, Ok(BlockStatus::StatusDisqualifiedFromChain)));
        assert_eq!(consensus.lkg_virtual_state.load().daa_score, 2);
        index += 1;
    }
    // // Add one more block to reach activation score
    consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![(index - 1).into()], vec![]).await.unwrap();
    index += 1;

    // Test 2: Verify the same transaction is accepted after activation
    let status = consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![index.into()], vec![tx.clone()]).await;
    assert!(matches!(status, Ok(BlockStatus::StatusUTXOValid)));
    assert!(consensus.lkg_virtual_state.load().accepted_tx_ids.contains(&tx_id));
    consensus.shutdown(wait_handles);
    drop(consensus);
}

#[tokio::test]
async fn payload_test() {
    let config = ConfigBuilder::new(DEVNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.prior_coinbase_maturity = 0;
            p.crescendo.coinbase_maturity = 0;
            p.crescendo_activation = ForkActivation::always()
        })
        .build();
    let consensus = TestConsensus::new(&config);
    let wait_handles = consensus.init();

    let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![OpTrue]), vec![]);
    let b = consensus.build_utxo_valid_block_with_parents(1.into(), vec![config.genesis.hash], miner_data.clone(), vec![]);
    consensus.validate_and_insert_block(b.to_immutable()).virtual_state_task.await.unwrap();
    let funding_block = consensus.build_utxo_valid_block_with_parents(2.into(), vec![1.into()], miner_data, vec![]);
    let (cb_id, cb_amount) = {
        let mut cb = funding_block.transactions[0].clone();
        cb.finalize();
        (cb.id(), cb.outputs[0].value)
    };

    consensus.validate_and_insert_block(funding_block.to_immutable()).virtual_state_task.await.unwrap();
    let mut txx = Transaction::new(
        0,
        vec![TransactionInput::new(TransactionOutpoint { transaction_id: cb_id, index: 0 }, vec![], 0, 0)],
        vec![TransactionOutput::new(cb_amount / 2, ScriptPublicKey::default())],
        0,
        SubnetworkId::default(),
        0,
        vec![0; (config.params.max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR / 2) as usize],
    );

    // Create a tx with transient mass over the block limit
    txx.payload = vec![0; (2 * config.params.max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR) as usize];
    let mut tx = MutableTransaction::from_tx(txx.clone());
    // This triggers storage mass population
    consensus.validate_mempool_transaction(&mut tx, &TransactionValidationArgs::default()).unwrap();
    let consensus_res = consensus.add_utxo_valid_block_with_parents(4.into(), vec![2.into()], vec![tx.tx.unwrap_or_clone()]).await;
    assert_match!(consensus_res, Err(RuleError::ExceedsTransientMassLimit(_, _)));

    // Fix the payload to be below the limit
    txx.payload = vec![0; (config.params.max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR / 2) as usize];
    let mut tx = MutableTransaction::from_tx(txx.clone());
    // This triggers storage mass population
    consensus.validate_mempool_transaction(&mut tx, &TransactionValidationArgs::default()).unwrap();
    let status = consensus.add_utxo_valid_block_with_parents(3.into(), vec![2.into()], vec![tx.tx.unwrap_or_clone()]).await;
    assert!(matches!(status, Ok(BlockStatus::StatusUTXOValid)));

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn payload_activation_test() {
    use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;

    // Set payload activation at DAA score 3 for this test
    const PAYLOAD_ACTIVATION_DAA_SCORE: u64 = 3;

    init_allocator_with_default_settings();

    // Create initial UTXO to fund our test transactions
    let initial_utxo_collection = [(
        TransactionOutpoint::new(1.into(), 0),
        UtxoEntry {
            amount: SOMPI_PER_KASPA,
            script_public_key: ScriptPublicKey::from_vec(0, vec![OpTrue]),
            block_daa_score: 0,
            is_coinbase: false,
        },
    )];

    // Initialize consensus with payload activation point
    let config = ConfigBuilder::new(DEVNET_PARAMS)
        .skip_proof_of_work()
        .apply_args(|cfg| {
            let mut genesis_multiset = MuHash::new();
            initial_utxo_collection.iter().for_each(|(outpoint, utxo)| {
                genesis_multiset.add_utxo(outpoint, utxo);
            });
            cfg.params.genesis.utxo_commitment = genesis_multiset.finalize();
            let genesis_header: Header = (&cfg.params.genesis).into();
            cfg.params.genesis.hash = genesis_header.hash;
        })
        .edit_consensus_params(|p| {
            p.crescendo_activation = ForkActivation::new(PAYLOAD_ACTIVATION_DAA_SCORE);
        })
        .build();

    let consensus = TestConsensus::new(&config);
    let mut genesis_multiset = MuHash::new();
    consensus.append_imported_pruning_point_utxos(&initial_utxo_collection, &mut genesis_multiset);
    consensus.import_pruning_point_utxo_set(config.genesis.hash, genesis_multiset).unwrap();
    let wait_handles = consensus.init();

    // Build blockchain up to one block before activation
    let mut index = 0;
    for _ in 0..PAYLOAD_ACTIVATION_DAA_SCORE - 1 {
        let parent = if index == 0 { config.genesis.hash } else { index.into() };
        consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![parent], vec![]).await.unwrap();
        index += 1;
    }
    assert_eq!(consensus.get_virtual_daa_score(), index);

    // Create transaction with large payload
    let large_payload = vec![0u8; (config.params.max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR / 2) as usize];
    let mut tx_with_payload = Transaction::new(
        0,
        vec![TransactionInput::new(
            initial_utxo_collection[0].0,
            vec![], // Empty signature script since we're using OpTrue
            0,
            0,
        )],
        vec![TransactionOutput::new(initial_utxo_collection[0].1.amount - 5000, ScriptPublicKey::from_vec(0, vec![OpTrue]))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        large_payload,
    );
    tx_with_payload.finalize();
    let tx_id = tx_with_payload.id();

    // Test 1: Build empty block, then manually insert invalid tx and verify consensus rejects it
    {
        let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);

        // First build block without transactions
        let mut block =
            consensus.build_utxo_valid_block_with_parents((index + 1).into(), vec![index.into()], miner_data.clone(), vec![]);

        let mut tx = MutableTransaction::from_tx(tx_with_payload.clone());
        // This triggers storage mass population
        let _ = consensus.validate_mempool_transaction(&mut tx, &TransactionValidationArgs::default());

        // Insert our test transaction and recalculate block hashes
        block.transactions.push(tx.tx.unwrap_or_clone());

        block.header.hash_merkle_root =
            calc_hash_merkle_root(block.transactions.iter(), config.crescendo_activation.is_active(block.header.daa_score));
        let block_status = consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await;
        assert!(matches!(block_status, Err(RuleError::TxInContextFailed(tx, TxRuleError::NonCoinbaseTxHasPayload)) if tx == tx_id));
        assert_eq!(consensus.lkg_virtual_state.load().daa_score, PAYLOAD_ACTIVATION_DAA_SCORE - 1);
        index += 1;
    }

    // Add one more block to reach activation score
    consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![(index - 1).into()], vec![]).await.unwrap();
    index += 1;

    let mut tx = MutableTransaction::from_tx(tx_with_payload.clone());
    // This triggers storage mass population
    let _ = consensus.validate_mempool_transaction(&mut tx, &TransactionValidationArgs::default());

    // Test 2: Verify the same transaction is accepted after activation
    let status =
        consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![index.into()], vec![tx.tx.unwrap_or_clone()]).await;

    assert!(matches!(status, Ok(BlockStatus::StatusUTXOValid)));
    assert!(consensus.lkg_virtual_state.load().accepted_tx_ids.contains(&tx_id));
    consensus.shutdown(wait_handles);
    drop(consensus);
}

#[tokio::test]
async fn runtime_sig_op_counting_test() {
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync, hashing::sighash_type::SIG_HASH_ALL, subnets::SUBNETWORK_ID_NATIVE,
    };
    use kaspa_txscript::{opcodes::codes::*, script_builder::ScriptBuilder};

    // Runtime sig op counting activates at DAA score 3
    const RUNTIME_SIGOP_ACTIVATION_DAA_SCORE: u64 = 3;

    init_allocator_with_default_settings();

    // Set up signing key for signature verification
    let secp = secp256k1::Secp256k1::new();
    let (secret_key, _) = secp.generate_keypair(&mut rand::thread_rng());
    let keypair = secp256k1::Keypair::from_seckey_slice(secp256k1::SECP256K1, &secret_key.secret_bytes()).unwrap();
    let pub_key = keypair.x_only_public_key().0.serialize();

    let reused_values = SigHashReusedValuesUnsync::new();

    // Create redeem script that has 1 sig op in the executed branch (true)
    // and 3 sig ops in the non-executed branch (false)
    let redeem_script = || -> ScriptBuilderResult<Vec<u8>> {
        Ok(ScriptBuilder::new()
            .add_op(OpTrue)?
            .add_op(OpIf)?
            .add_op(OpCheckSig)?     // This sig op gets executed
            .add_op(OpElse)?
            .add_op(OpCheckSig)?     // These sig ops are skipped
            .add_op(OpCheckSig)?
            .add_op(OpCheckSig)?
            .add_op(OpEndIf)?
            .drain())
    }()
    .unwrap();

    let script_pub_key = kaspa_txscript::pay_to_script_hash_script(&redeem_script);

    // Set up initial UTXO with P2SH script
    let initial_utxo_collection = [(
        TransactionOutpoint::new(1.into(), 0),
        UtxoEntry { amount: SOMPI_PER_KASPA, script_public_key: script_pub_key.clone(), block_daa_score: 0, is_coinbase: false },
    )];

    let config = ConfigBuilder::new(DEVNET_PARAMS)
        .skip_proof_of_work()
        .apply_args(|cfg| {
            let mut genesis_multiset = MuHash::new();
            initial_utxo_collection.iter().for_each(|(outpoint, utxo)| {
                genesis_multiset.add_utxo(outpoint, utxo);
            });
            cfg.params.genesis.utxo_commitment = genesis_multiset.finalize();
            let genesis_header: Header = (&cfg.params.genesis).into();
            cfg.params.genesis.hash = genesis_header.hash;
        })
        .edit_consensus_params(|p| {
            p.crescendo_activation = ForkActivation::new(RUNTIME_SIGOP_ACTIVATION_DAA_SCORE);
        })
        .build();

    let consensus = TestConsensus::new(&config);
    let mut genesis_multiset = MuHash::new();
    consensus.append_imported_pruning_point_utxos(&initial_utxo_collection, &mut genesis_multiset);
    consensus.import_pruning_point_utxo_set(config.genesis.hash, genesis_multiset).unwrap();
    let wait_handles = consensus.init();

    // Build blockchain up to one block before activation
    let mut index = 0;
    for _ in 0..RUNTIME_SIGOP_ACTIVATION_DAA_SCORE - 1 {
        let parent = if index == 0 { config.genesis.hash } else { index.into() };
        consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![parent], vec![]).await.unwrap();
        index += 1;
    }

    // Create transaction spending P2SH with 1 sig op limit
    let mut tx = Transaction::new(
        0,
        vec![TransactionInput::new(
            initial_utxo_collection[0].0,
            vec![], // Placeholder for signature script
            0,
            1, // Only allowing 1 sig op - important for test
        )],
        vec![TransactionOutput::new(initial_utxo_collection[0].1.amount - 5000, ScriptPublicKey::from_vec(0, vec![OpTrue]))],
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );

    // Sign transaction
    let mut tx_for_signing = MutableTransaction::new(tx.clone());
    tx_for_signing.entries = vec![Some(initial_utxo_collection[0].1.clone())];

    let signature = {
        let hash = calc_schnorr_signature_hash(&tx_for_signing.as_verifiable(), 0, SIG_HASH_ALL, &reused_values);
        let msg = secp256k1::Message::from_digest_slice(hash.as_bytes().as_slice()).unwrap();
        let sig = keypair.sign_schnorr(msg);
        let mut signature = sig.as_ref().to_vec();
        signature.push(SIG_HASH_ALL.to_u8());
        signature
    };

    // Complete transaction with signature script
    tx.inputs[0].signature_script =
        ScriptBuilder::new().add_data(&signature).unwrap().add_data(&pub_key).unwrap().add_data(&redeem_script).unwrap().drain();

    tx.finalize();

    let mut tx = MutableTransaction::from_tx(tx);
    // This triggers storage mass population
    let _ = consensus.validate_mempool_transaction(&mut tx, &TransactionValidationArgs::default());
    let tx = tx.tx.unwrap_or_clone();

    // Test 1: Before activation, tx should be rejected due to static sig op counting (sees 3 ops)
    {
        let miner_data = MinerData::new(ScriptPublicKey::from_vec(0, vec![]), vec![]);
        let mut block =
            consensus.build_utxo_valid_block_with_parents((index + 1).into(), vec![index.into()], miner_data.clone(), vec![]);
        block.transactions.push(tx.clone());
        block.header.hash_merkle_root =
            calc_hash_merkle_root(block.transactions.iter(), config.crescendo_activation.is_active(block.header.daa_score));
        let block_status = consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await;
        assert!(matches!(block_status, Ok(BlockStatus::StatusDisqualifiedFromChain)));
        index += 1;
    }

    // Add block to reach activation
    consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![(index - 1).into()], vec![]).await.unwrap();
    index += 1;

    // Test 2: After activation, tx should be accepted as runtime counting only sees 1 executed sig op
    let status = consensus.add_utxo_valid_block_with_parents((index + 1).into(), vec![index.into()], vec![tx]).await;
    assert!(matches!(status, Ok(BlockStatus::StatusUTXOValid)));
    consensus.shutdown(wait_handles);
    drop(consensus);
}
