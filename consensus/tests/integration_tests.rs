//!
//! Integration tests
//!

use consensus::consensus::test_consensus::{create_temp_db, TestConsensus};
use consensus::constants::BLOCK_VERSION;
use consensus::errors::{BlockProcessResult, RuleError};
use consensus::model::stores::ghostdag::{GhostdagStoreReader, KType as GhostdagKType};
use consensus::model::stores::headers::HeaderStoreReader;
use consensus::model::stores::reachability::DbReachabilityStore;
use consensus::params::{Params, DEVNET_PARAMS, MAINNET_PARAMS};
use consensus::processes::reachability::tests::{DagBlock, DagBuilder, StoreValidationExtensions};
use consensus_core::block::Block;
use consensus_core::blockhash::new_unique;
use consensus_core::blockstatus::BlockStatus;
use consensus_core::header::Header;
use consensus_core::subnets::SubnetworkId;
use consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use consensus_core::{blockhash, hashing, BlueWorkType};
use hashes::Hash;

use flate2::read::GzDecoder;
use futures_util::future::join_all;
use itertools::Itertools;
use math::Uint256;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::{
    collections::HashMap,
    fs::File,
    future::Future,
    io::{self, BufRead, BufReader},
    str::{from_utf8, FromStr},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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
            src.parents.iter().map(|id| (id.parse::<u64>().unwrap() + 1).into()).collect(),
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
    let file = common::open_file(&path_str);
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
    let (_temp_db_lifetime, db) = create_temp_db();
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
            builder.store().validate_intervals(new_hash).unwrap();
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

#[tokio::test]
async fn consensus_sanity_test() {
    let genesis_child: Hash = 2.into();

    let consensus = TestConsensus::create_from_temp_db(&MAINNET_PARAMS.clone_with_skip_pow());
    let wait_handles = consensus.init();

    consensus
        .validate_and_insert_block(consensus.build_block_with_parents(genesis_child, vec![MAINNET_PARAMS.genesis_hash]).to_immutable())
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
    let mut path_strings: Vec<String> =
        common::read_dir("tests/testdata/dags").map(|f| f.unwrap().path().to_str().unwrap().to_owned()).collect();
    path_strings.sort();

    for path_str in path_strings.iter() {
        println!("Running test {}", path_str);
        let file = File::open(path_str).unwrap();
        let reader = BufReader::new(file);
        let test: GhostdagTestDag = serde_json::from_reader(reader).unwrap();

        let mut params = MAINNET_PARAMS.clone_with_skip_pow();
        params.genesis_hash = string_to_hash(&test.genesis_id);
        params.ghostdag_k = test.k;

        let consensus = TestConsensus::create_from_temp_db(&params);
        let wait_handles = consensus.init();

        for block in test.blocks.iter() {
            println!("Processing block {}", block.id);
            let block_id = string_to_hash(&block.id);
            let block_header = consensus.build_header_with_parents(block_id, strings_to_hashes(&block.parents));

            // Submit to consensus
            consensus.validate_and_insert_block(Block::from_header(block_header)).await.unwrap();
        }

        // Clone with a new cache in order to verify correct writes to the DB itself
        let ghostdag_store = consensus.ghostdag_store().clone_with_new_cache(10000);

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
    let (_temp_db_lifetime, db) = create_temp_db();

    let mut params = MAINNET_PARAMS.clone_with_skip_pow();
    params.genesis_hash = string_to_hash("A");
    params.ghostdag_k = 1;

    let consensus = TestConsensus::new(db, &params);
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
        println!("Processing block {}", test_block.id);
        let block_id = string_to_hash(test_block.id);
        let block = consensus.build_block_with_parents(
            block_id,
            strings_to_hashes(&test_block.parents.iter().map(|parent| String::from(*parent)).collect()),
        );

        // Submit to consensus
        consensus.validate_and_insert_block(block.to_immutable()).await.unwrap();

        let window = consensus.dag_traversal_manager().block_window(&consensus.ghostdag_store().get_data(block_id).unwrap(), 10);

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
    let params = &MAINNET_PARAMS;
    let consensus = TestConsensus::create_from_temp_db(params);
    let wait_handles = consensus.init();
    let block = consensus.build_block_with_parents(1.into(), vec![params.genesis_hash]);

    {
        let mut block = block.clone();
        let block_version = BLOCK_VERSION - 1;
        block.header.version = block_version;
        match consensus.validate_and_insert_block(block.to_immutable()).await {
            Err(RuleError::WrongBlockVersion(wrong_version)) => {
                assert_eq!(wrong_version, block_version)
            }
            res => {
                panic!("Unexpected result: {:?}", res)
            }
        }
    }

    {
        let mut block = block.clone();
        block.header.hash = 2.into();

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let block_ts = now + params.timestamp_deviation_tolerance * params.target_time_per_block + 2000;
        block.header.timestamp = block_ts;
        match consensus.validate_and_insert_block(block.to_immutable()).await {
            Err(RuleError::TimeTooFarIntoTheFuture(ts, _)) => {
                assert_eq!(ts, block_ts)
            }
            res => {
                panic!("Unexpected result: {:?}", res)
            }
        }
    }

    {
        let mut block = block.clone();
        block.header.hash = 3.into();
        block.header.parents_by_level[0] = vec![];
        match consensus.validate_and_insert_block(block.to_immutable()).await {
            Err(RuleError::NoParents) => {}
            res => {
                panic!("Unexpected result: {:?}", res)
            }
        }
    }

    {
        let mut block = block.clone();
        block.header.hash = 4.into();
        block.header.parents_by_level[0] = (5..(params.max_block_parents + 6)).map(|x| (x as u64).into()).collect();
        match consensus.validate_and_insert_block(block.to_immutable()).await {
            Err(RuleError::TooManyParents(num_parents, limit)) => {
                assert_eq!((params.max_block_parents + 1) as usize, num_parents);
                assert_eq!(limit, params.max_block_parents as usize);
            }
            res => {
                panic!("Unexpected result: {:?}", res)
            }
        }
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn incest_test() {
    let params = MAINNET_PARAMS.clone_with_skip_pow();
    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();
    let block = consensus.build_block_with_parents(1.into(), vec![params.genesis_hash]);
    consensus.validate_and_insert_block(block.to_immutable()).await.unwrap();

    let mut block = consensus.build_block_with_parents(2.into(), vec![params.genesis_hash]);
    block.header.parents_by_level[0] = vec![1.into(), params.genesis_hash];
    match consensus.validate_and_insert_block(block.to_immutable()).await {
        Err(RuleError::InvalidParentsRelation(a, b)) => {
            assert_eq!(a, params.genesis_hash);
            assert_eq!(b, 1.into());
        }
        res => {
            panic!("Unexpected result: {:?}", res)
        }
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn missing_parents_test() {
    let params = &MAINNET_PARAMS;
    let consensus = TestConsensus::create_from_temp_db(params);
    let wait_handles = consensus.init();
    let mut block = consensus.build_block_with_parents(1.into(), vec![params.genesis_hash]);
    block.header.parents_by_level[0] = vec![0.into()];
    match consensus.validate_and_insert_block(block.to_immutable()).await {
        Err(RuleError::MissingParents(missing)) => {
            assert_eq!(missing, vec![0.into()]);
        }
        res => {
            panic!("Unexpected result: {:?}", res)
        }
    }

    consensus.shutdown(wait_handles);
}

// Errors such as ErrTimeTooOld which happen after DAA and PoW validation should set the block
// as a known invalid.
#[tokio::test]
async fn known_invalid_test() {
    let params = MAINNET_PARAMS.clone_with_skip_pow();
    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();
    let mut block = consensus.build_block_with_parents(1.into(), vec![params.genesis_hash]);
    block.header.timestamp -= 1;

    match consensus.validate_and_insert_block(block.clone().to_immutable()).await {
        Err(RuleError::TimeTooOld(_, _)) => {}
        res => {
            panic!("Unexpected result: {:?}", res)
        }
    }

    match consensus.validate_and_insert_block(block.to_immutable()).await {
        Err(RuleError::KnownInvalid) => {}
        res => {
            panic!("Unexpected result: {:?}", res)
        }
    }

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn median_time_test() {
    let params = MAINNET_PARAMS.clone_with_skip_pow();
    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();

    let num_blocks = 2 * params.timestamp_deviation_tolerance - 1;
    for i in 1..(num_blocks + 1) {
        let parent = if i == 1 { params.genesis_hash } else { (i - 1).into() };
        let mut block = consensus.build_block_with_parents(i.into(), vec![parent]);
        block.header.timestamp = params.genesis_timestamp + i;
        consensus.validate_and_insert_block(block.to_immutable()).await.unwrap();
    }

    let mut block = consensus.build_block_with_parents((num_blocks + 2).into(), vec![num_blocks.into()]);
    // We set the timestamp to be less than the median time and expect the block to be rejected
    block.header.timestamp = params.genesis_timestamp + num_blocks - params.timestamp_deviation_tolerance - 1;
    match consensus.validate_and_insert_block(block.to_immutable()).await {
        Err(RuleError::TimeTooOld(_, _)) => {}
        res => {
            panic!("Unexpected result: {:?}", res)
        }
    }

    let mut block = consensus.build_block_with_parents((num_blocks + 3).into(), vec![num_blocks.into()]);
    // We set the timestamp to be the exact median time and expect the block to be rejected
    block.header.timestamp = params.genesis_timestamp + num_blocks - params.timestamp_deviation_tolerance;
    match consensus.validate_and_insert_block(block.to_immutable()).await {
        Err(RuleError::TimeTooOld(_, _)) => {}
        res => {
            panic!("Unexpected result: {:?}", res)
        }
    }

    let mut block = consensus.build_block_with_parents((num_blocks + 4).into(), vec![(num_blocks).into()]);
    // We set the timestamp to be bigger than the median time and expect the block to be inserted successfully.
    block.header.timestamp = params.genesis_timestamp + params.timestamp_deviation_tolerance + 1;
    consensus.validate_and_insert_block(block.to_immutable()).await.unwrap();

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn mergeset_size_limit_test() {
    let params = MAINNET_PARAMS.clone_with_skip_pow();
    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();

    let num_blocks_per_chain = params.mergeset_size_limit + 1;

    let mut tip1_hash = params.genesis_hash;
    for i in 1..(num_blocks_per_chain + 1) {
        let block = consensus.build_block_with_parents(i.into(), vec![tip1_hash]);
        tip1_hash = block.header.hash;
        consensus.validate_and_insert_block(block.to_immutable()).await.unwrap();
    }

    let mut tip2_hash = params.genesis_hash;
    for i in (num_blocks_per_chain + 2)..(2 * num_blocks_per_chain + 1) {
        let block = consensus.build_block_with_parents(i.into(), vec![tip2_hash]);
        tip2_hash = block.header.hash;
        consensus.validate_and_insert_block(block.to_immutable()).await.unwrap();
    }

    let block = consensus.build_block_with_parents((3 * num_blocks_per_chain + 1).into(), vec![tip1_hash, tip2_hash]);
    match consensus.validate_and_insert_block(block.to_immutable()).await {
        Err(RuleError::MergeSetTooBig(a, b)) => {
            assert_eq!(a, params.mergeset_size_limit + 1);
            assert_eq!(b, params.mergeset_size_limit);
        }
        res => {
            panic!("Unexpected result: {:?}", res)
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
}

impl KaspadGoParams {
    fn into_params(self, genesis_header: &Header) -> Params {
        let finality_depth = self.FinalityDuration / self.TargetTimePerBlock;
        Params {
            genesis_hash: genesis_header.hash,
            ghostdag_k: self.K,
            timestamp_deviation_tolerance: self.TimestampDeviationTolerance,
            target_time_per_block: self.TargetTimePerBlock / 1_000_000,
            max_block_parents: self.MaxBlockParents,
            difficulty_window_size: self.DifficultyAdjustmentWindowSize,
            genesis_timestamp: genesis_header.timestamp,
            genesis_bits: genesis_header.bits,
            mergeset_size_limit: self.MergeSetSizeLimit,
            merge_depth: self.MergeDepth,
            finality_depth,
            pruning_depth: 2 * finality_depth + 4 * self.MergeSetSizeLimit * self.K as u64 + 2 * self.K as u64 + 2,
            coinbase_payload_script_public_key_max_len: self.CoinbasePayloadScriptPublicKeyMaxLength,
            max_coinbase_payload_len: self.MaxCoinbasePayloadLength,
            max_tx_inputs: MAINNET_PARAMS.max_tx_inputs,
            max_tx_outputs: MAINNET_PARAMS.max_tx_outputs,
            max_signature_script_len: MAINNET_PARAMS.max_signature_script_len,
            max_script_public_key_len: MAINNET_PARAMS.max_script_public_key_len,
            mass_per_tx_byte: self.MassPerTxByte,
            mass_per_script_pub_key_byte: self.MassPerScriptPubKeyByte,
            mass_per_sig_op: self.MassPerSigOp,
            max_block_mass: self.MaxBlockMass,
            deflationary_phase_daa_score: self.DeflationaryPhaseDaaScore,
            pre_deflationary_phase_base_subsidy: self.PreDeflationaryPhaseBaseSubsidy,
            coinbase_maturity: MAINNET_PARAMS.coinbase_maturity,
            skip_proof_of_work: self.SkipProofOfWork,
            max_block_level: self.MaxBlockLevel,
        }
    }
}

#[tokio::test]
async fn goref_custom_pruning_depth() {
    json_test("tests/testdata/goref_custom_pruning_depth.json.gz").await
}

#[tokio::test]
async fn goref_notx_test() {
    json_test("tests/testdata/goref-notx-5000-blocks.json.gz").await
}

#[tokio::test]
async fn goref_notx_concurrent_test() {
    json_concurrency_test("tests/testdata/goref-notx-5000-blocks.json.gz").await
}

#[tokio::test]
async fn goref_tx_small_test() {
    json_test("tests/testdata/goref-905-tx-265-blocks.json.gz").await
}

#[tokio::test]
async fn goref_tx_small_concurrent_test() {
    json_concurrency_test("tests/testdata/goref-905-tx-265-blocks.json.gz").await
}

#[ignore]
#[tokio::test]
async fn goref_tx_big_test() {
    // TODO: add this file to a data repo and fetch dynamically
    json_test("tests/testdata/goref-1.6M-tx-10K-blocks.json.gz").await
}

#[ignore]
#[tokio::test]
async fn goref_tx_big_concurrent_test() {
    // TODO: add this file to a data repo and fetch dynamically
    json_concurrency_test("tests/testdata/goref-1.6M-tx-10K-blocks.json.gz").await
}

async fn json_test(file_path: &str) {
    let file = common::open_file(file_path);
    let decoder = GzDecoder::new(file);
    let mut lines = BufReader::new(decoder).lines();
    let first_line = lines.next().unwrap().unwrap();
    let go_params_res: Result<KaspadGoParams, _> = serde_json::from_str(&first_line);
    let params = if let Ok(go_params) = go_params_res {
        let second_line = lines.next().unwrap().unwrap();
        let genesis = json_line_to_block(second_line);
        go_params.into_params(&genesis.header)
    } else {
        let genesis = json_line_to_block(first_line);
        let mut params = DEVNET_PARAMS;
        params.genesis_bits = genesis.header.bits;
        params.genesis_hash = genesis.header.hash;
        params.genesis_timestamp = genesis.header.timestamp;
        params
    };

    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();

    let mut last_time = SystemTime::now();
    let mut last_index: usize = 0;
    for (i, line) in lines.enumerate() {
        let now = SystemTime::now();
        let passed = now.duration_since(last_time).unwrap();
        if passed > Duration::new(10, 0) {
            println!("Processed {} blocks in the last {} seconds", i - last_index, passed.as_secs());
            last_time = now;
            last_index = i;
        }
        let block = json_line_to_block(line.unwrap());
        let hash = block.header.hash;
        // Test our hashing implementation vs the hash accepted from the json source
        assert_eq!(hashing::header::hash(&block.header), hash, "header hashing for block {} {} failed", i, hash);
        let status = consensus.validate_and_insert_block(block).await.unwrap_or_else(|e| panic!("block {} {} failed: {}", i, hash, e));
        assert!(status.is_utxo_valid_or_pending());
    }

    // Assert that at least one body tip was resolved with valid UTXO
    assert!(consensus.body_tips().iter().copied().any(|h| consensus.block_status(h) == BlockStatus::StatusUTXOValid));

    consensus.shutdown(wait_handles);
}

async fn json_concurrency_test(file_path: &str) {
    let file = common::open_file(file_path);
    let decoder = GzDecoder::new(file);
    let mut lines = io::BufReader::new(decoder).lines();
    let first_line = lines.next().unwrap();
    let genesis = json_line_to_block(first_line.unwrap());
    let mut params = DEVNET_PARAMS;
    params.genesis_bits = genesis.header.bits;
    params.genesis_hash = genesis.header.hash;
    params.genesis_timestamp = genesis.header.timestamp;

    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();

    let chunks = lines.into_iter().chunks(1000);
    let mut iter = chunks.into_iter();
    let mut chunk = iter.next().unwrap();
    let mut prev_joins = submit_chunk(&consensus, &mut chunk);

    for mut chunk in iter {
        let current_joins = submit_chunk(&consensus, &mut chunk);
        let statuses = join_all(prev_joins).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>().unwrap();
        assert!(statuses.iter().all(|s| s.is_utxo_valid_or_pending()));
        prev_joins = current_joins;
    }

    let statuses = join_all(prev_joins).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>().unwrap();
    assert!(statuses.iter().all(|s| s.is_utxo_valid_or_pending()));

    // Assert that at least one body tip was resolved with valid UTXO
    assert!(consensus.body_tips().iter().copied().any(|h| consensus.block_status(h) == BlockStatus::StatusUTXOValid));

    consensus.shutdown(wait_handles);
}

fn submit_chunk(
    consensus: &TestConsensus,
    chunk: &mut impl Iterator<Item = std::io::Result<String>>,
) -> Vec<impl Future<Output = BlockProcessResult<BlockStatus>>> {
    let mut futures = Vec::new();
    for line in chunk {
        let f = consensus.validate_and_insert_block(json_line_to_block(line.unwrap()));
        futures.push(f);
    }
    futures
}

fn json_line_to_block(line: String) -> Block {
    let rpc_block: RPCBlock = serde_json::from_str(&line).unwrap();
    Block::new(
        Header {
            hash: Hash::from_str(&rpc_block.VerboseData.Hash).unwrap(),
            version: rpc_block.Header.Version,
            parents_by_level: rpc_block
                .Header
                .Parents
                .iter()
                .map(|item| item.ParentHashes.iter().map(|parent| Hash::from_str(parent).unwrap()).collect())
                .collect(),
            hash_merkle_root: Hash::from_str(&rpc_block.Header.HashMerkleRoot).unwrap(),
            accepted_id_merkle_root: Hash::from_str(&rpc_block.Header.AcceptedIDMerkleRoot).unwrap(),
            utxo_commitment: Hash::from_str(&rpc_block.Header.UTXOCommitment).unwrap(),
            timestamp: rpc_block.Header.Timestamp,
            bits: rpc_block.Header.Bits,
            nonce: rpc_block.Header.Nonce,
            daa_score: rpc_block.Header.DAAScore,
            blue_work: BlueWorkType::from_hex(&rpc_block.Header.BlueWork).unwrap(),
            blue_score: rpc_block.Header.BlueScore,
            pruning_point: Hash::from_str(&rpc_block.Header.PruningPoint).unwrap(),
        },
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
    let mut params = MAINNET_PARAMS.clone_with_skip_pow();
    params.ghostdag_k = 5;
    params.merge_depth = 7;

    assert!((params.ghostdag_k as u64) < params.merge_depth, "K must be smaller than merge depth for this test to run");

    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();

    let mut selected_chain = vec![params.genesis_hash];
    for i in 1..(params.merge_depth + 3) {
        let hash: Hash = (i + 1).into();
        consensus.add_block_with_parents(hash, vec![*selected_chain.last().unwrap()]).await.unwrap();
        selected_chain.push(hash);
    }

    // The length of block_chain_2 is shorter by one than selected_chain, so selected_chain will remain the selected chain.
    let mut block_chain_2 = vec![params.genesis_hash];
    for i in 1..(params.merge_depth + 2) {
        let hash: Hash = (i + params.merge_depth + 3).into();
        consensus.add_block_with_parents(hash, vec![*block_chain_2.last().unwrap()]).await.unwrap();
        block_chain_2.push(hash);
    }

    // The merge depth root belongs to selected_chain, and block_chain_2[1] is red and doesn't have it in its past, and is not in the
    // past of any kosherizing block, so we expect the next block to be rejected.
    match consensus.add_block_with_parents(100.into(), vec![block_chain_2[1], *selected_chain.last().unwrap()]).await {
        Err(RuleError::ViolatingBoundedMergeDepth) => {}
        res => panic!("Unexpected result: {:?}", res),
    }

    // A block that points to tip of both chains will be rejected for similar reasons (since block_chain_2 tip is also red).
    match consensus.add_block_with_parents(101.into(), vec![*block_chain_2.last().unwrap(), *selected_chain.last().unwrap()]).await {
        Err(RuleError::ViolatingBoundedMergeDepth) => {}
        res => panic!("Unexpected result: {:?}", res),
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
    for i in 0..params.ghostdag_k {
        let hash = Hash::from_u64_word(i as u64 * 1000);
        consensus.add_block_with_parents(hash, vec![*selected_chain.last().unwrap()]).await.unwrap();
        selected_chain.push(hash);
    }

    // Since kosherizing_hash is now red, we expect this to fail.
    match consensus.add_block_with_parents(1100.into(), vec![kosherizing_hash, *selected_chain.last().unwrap()]).await {
        Err(RuleError::ViolatingBoundedMergeDepth) => {}
        res => panic!("Unexpected result: {:?}", res),
    }

    // point_at_blue_kosherizing is kosherizing kosherizing_hash, so this should pass.
    consensus.add_block_with_parents(1101.into(), vec![point_at_blue_kosherizing, *selected_chain.last().unwrap()]).await.unwrap();

    consensus.shutdown(wait_handles);
}

#[tokio::test]
async fn difficulty_test() {
    async fn add_block(consensus: &TestConsensus, block_time: Option<u64>, parents: Vec<Hash>) -> Header {
        let selected_parent = consensus.ghostdag_manager().find_selected_parent(&mut parents.iter().copied());
        let block_time = block_time.unwrap_or_else(|| {
            consensus.headers_store().get_timestamp(selected_parent).unwrap() + consensus.params.target_time_per_block
        });
        let mut header = consensus.build_header_with_parents(new_unique(), parents);
        header.timestamp = block_time;
        consensus.validate_and_insert_block(Block::new(header.clone(), vec![])).await.unwrap();
        header
    }

    async fn add_block_with_min_time(consensus: &TestConsensus, parents: Vec<Hash>) -> Header {
        let ghostdag_data = consensus.ghostdag_manager().ghostdag(&parents[..]);
        let (pmt, _) = consensus.past_median_time_manager().calc_past_median_time(&ghostdag_data);
        add_block(consensus, Some(pmt + 1), parents).await
    }

    fn compare_bits(a: u32, b: u32) -> Ordering {
        Uint256::from_compact_target_bits(a).cmp(&Uint256::from_compact_target_bits(b))
    }

    let mut params = MAINNET_PARAMS.clone_with_skip_pow();
    params.ghostdag_k = 1;
    params.difficulty_window_size = 140;

    let consensus = TestConsensus::create_from_temp_db(&params);
    let wait_handles = consensus.init();

    let fake_genesis = Header {
        hash: params.genesis_hash,
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

    let mut tip = fake_genesis;
    for _ in 0..params.difficulty_window_size {
        tip = add_block(&consensus, None, vec![tip.hash]).await;
        assert_eq!(tip.bits, params.genesis_bits, "until first DAA window is created difficulty should remains unchanged");
    }

    for _ in 0..params.difficulty_window_size + 10 {
        tip = add_block(&consensus, None, vec![tip.hash]).await;
        assert_eq!(tip.bits, params.genesis_bits, "block rate wasn't changed so difficulty is not expected to change");
    }

    let block_in_the_past = add_block_with_min_time(&consensus, vec![tip.hash]).await;
    assert_eq!(
        block_in_the_past.bits, params.genesis_bits,
        "block_in_the_past shouldn't affect its own difficulty, but only its future"
    );
    tip = block_in_the_past;
    tip = add_block(&consensus, None, vec![tip.hash]).await;
    assert_eq!(tip.bits, 0x1d02c50f); // TODO: Check that it makes sense

    // Increase block rate to increase difficulty
    for _ in 0..params.difficulty_window_size {
        let prev_bits = tip.bits;
        tip = add_block_with_min_time(&consensus, vec![tip.hash]).await;
        assert!(
            compare_bits(tip.bits, prev_bits) != Ordering::Greater,
            "Because we're increasing the block rate, the difficulty can't decrease"
        );
    }

    // Add blocks until difficulty stabilizes
    let mut same_bits_count = 0;
    while same_bits_count < params.difficulty_window_size + 1 {
        let prev_bits = tip.bits;
        tip = add_block(&consensus, None, vec![tip.hash]).await;
        if tip.bits == prev_bits {
            same_bits_count += 1;
        } else {
            same_bits_count = 0;
        }
    }

    let slow_block_time = tip.timestamp + params.target_time_per_block + 1000;
    let slow_block = add_block(&consensus, Some(slow_block_time), vec![tip.hash]).await;
    let slow_block_bits = slow_block.bits;
    assert_eq!(slow_block.bits, tip.bits, "The difficulty should change only when slow_block is in the past");

    tip = slow_block;
    tip = add_block(&consensus, None, vec![tip.hash]).await;
    assert_eq!(
        compare_bits(tip.bits, slow_block_bits),
        Ordering::Greater,
        "block rate was decreased due to slow_block, so we expected difficulty to be reduced"
    );

    // Here we create two chains: a chain of blue blocks, and a chain of red blocks with
    // very low timestamps. Because the red blocks should be part of the difficulty
    // window, their low timestamps should lower the difficulty, and we check it by
    // comparing the bits of two blocks with the same blue score, one with the red
    // blocks in its past and one without.
    let split_hash = tip.hash;
    let mut blue_tip_hash = split_hash;
    for _ in 0..params.difficulty_window_size {
        blue_tip_hash = add_block(&consensus, None, vec![blue_tip_hash]).await.hash;
    }

    let split_hash = tip.hash;
    let mut red_tip_hash = split_hash;
    const RED_CHAIN_LEN: usize = 10;
    for _ in 0..RED_CHAIN_LEN {
        red_tip_hash = add_block(&consensus, None, vec![red_tip_hash]).await.hash;
    }

    let tip_with_red_past = add_block(&consensus, None, vec![red_tip_hash, blue_tip_hash]).await;
    let tip_without_red_past = add_block(&consensus, None, vec![blue_tip_hash]).await;
    assert_eq!(
        compare_bits(tip_with_red_past.bits, tip_without_red_past.bits),
        Ordering::Less,
        "we expect the red blocks to increase the difficulty of tip_with_red_past"
    );

    // We repeat the test, but now we make the blue chain longer in order to filter
    // out the red blocks from the window, and check that the red blocks don't
    // affect the difficulty.
    blue_tip_hash = split_hash;
    for _ in 0..params.difficulty_window_size + RED_CHAIN_LEN + 1 {
        blue_tip_hash = add_block(&consensus, None, vec![blue_tip_hash]).await.hash;
    }

    red_tip_hash = split_hash;
    for _ in 0..RED_CHAIN_LEN {
        red_tip_hash = add_block(&consensus, None, vec![red_tip_hash]).await.hash;
    }

    let tip_with_red_past = add_block(&consensus, None, vec![red_tip_hash, blue_tip_hash]).await;
    let tip_without_red_past = add_block(&consensus, None, vec![blue_tip_hash]).await;
    assert_eq!(
        tip_with_red_past.bits, tip_without_red_past.bits,
        "we expect the red blocks to not affect the difficulty of tip_with_red_past"
    );

    consensus.shutdown(wait_handles);
}
