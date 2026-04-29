use std::sync::OnceLock;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, SamplingMode, black_box, criterion_group, criterion_main};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::config::params::MAINNET_PARAMS;
use kaspa_consensus_core::hashing::sighash::{SigHashReusedValuesSync, SigHashReusedValuesUnsync, calc_schnorr_signature_hash};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::mass::{ComputeBudget, Gram, MassCalculator, ScriptUnits};
use kaspa_consensus_core::tx::{
    MutableTransaction, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
    TransactionOutput, TxInputMass, UtxoEntry, VerifiableTransaction,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::opcodes::codes::{self, OpDrop, OpDup};
use kaspa_txscript::script_builder::ScriptBuilder;
use kaspa_txscript::{
    EngineCtx, EngineFlags, TxScriptEngine, pay_to_address_script, pay_to_script_hash_script, pay_to_script_hash_signature_script,
    zk_precompiles::tests::helpers::build_stark_script,
};
use kaspa_txscript_errors::TxScriptError;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use secp256k1::{Keypair, Message, Secp256k1, SecretKey};
use smallvec::SmallVec;

const BLOCK_COMPUTE_MASS_LIMIT: u64 = 500_000;
const BLOCK_TRANSIENT_MASS_LIMIT: u64 = 1_000_000;
const ADVERSARIAL_SLICE_LEN: i64 = 64;
const ADVERSARIAL_SIGSCRIPT_DATA_LEN: usize = 192;
const ADVERSARIAL_OUTPUT_SPK_DATA_LEN: usize = 160;
const ADVERSARIAL_PAYLOAD_LEN: usize = 256;
const ADVERSARIAL_ROUNDS: usize = 12;
const HASHING_SIGSCRIPT_DATA_LEN: usize = 10_000;
const HASHING_KEY_LEN: usize = 32;
const HASHING_ROUNDS: usize = 20;
const LARGE_PUSH_DUP_CAT_DATA_LEN_UPPER_BOUND: usize = 124_923;
const OP_DUP_BASE_DUP_COUNT: usize = 243;
const OP_DUP_FREE_BUDGET_DUP_COUNT: usize = 1107;
const OP_DUP_ONE_TX_PAIR_SEARCH_STEP: usize = 20;

struct BenchTx {
    tx: MutableTransaction<Transaction>,
}

struct BenchBlock {
    name: &'static str,
    txs: Vec<BenchTx>,
    tx_count: usize,
    input_count: usize,
    compute_mass: u64,
    transient_mass: u64,
}

type TxBuilder = fn(u32) -> (Transaction, Vec<UtxoEntry>);
type RoundTxBuilder = fn(u32, usize) -> (Transaction, Vec<UtxoEntry>);

fn pricing_flags(covenants_enabled: bool) -> EngineFlags {
    EngineFlags { covenants_enabled, sigop_script_units: Gram(1000).into() }
}

fn stack_entry_inline_capacity() -> usize {
    let entry: SmallVec<[u8; 8]> = SmallVec::new();
    entry.inline_size()
}

fn op_dup_free_budget_element_len() -> usize {
    stack_entry_inline_capacity() + 1
}

fn format_average_input_budget(block: &BenchBlock) -> String {
    let mut total_compute_budget = 0u64;
    let mut compute_budget_inputs = 0usize;
    let mut total_sigops = 0u64;
    let mut sigop_inputs = 0usize;

    for bench_tx in &block.txs {
        for input in &bench_tx.tx.tx.inputs {
            match input.mass {
                TxInputMass::ComputeBudget(budget) => {
                    total_compute_budget += u16::from(budget) as u64;
                    compute_budget_inputs += 1;
                }
                TxInputMass::SigopCount(count) => {
                    total_sigops += u8::from(count) as u64;
                    sigop_inputs += 1;
                }
            }
        }
    }

    if compute_budget_inputs > 0 && sigop_inputs == 0 {
        format!("avg_compute_budget {:.2}", total_compute_budget as f64 / compute_budget_inputs as f64)
    } else if sigop_inputs > 0 && compute_budget_inputs == 0 {
        format!("avg_sigop_count {:.2}", total_sigops as f64 / sigop_inputs as f64)
    } else if compute_budget_inputs > 0 || sigop_inputs > 0 {
        format!(
            "avg_compute_budget {:.2} avg_sigop_count {:.2}",
            total_compute_budget as f64 / compute_budget_inputs.max(1) as f64,
            total_sigops as f64 / sigop_inputs.max(1) as f64
        )
    } else {
        "avg_input_budget n/a".to_string()
    }
}

fn mass_calculator() -> &'static MassCalculator {
    static CALCULATOR: OnceLock<MassCalculator> = OnceLock::new();
    CALCULATOR.get_or_init(|| MassCalculator::new_with_consensus_params(&MAINNET_PARAMS))
}

fn compute_mass(tx: &Transaction) -> u64 {
    mass_calculator().calc_non_contextual_masses(tx).compute_mass
}

fn transient_mass(tx: &Transaction) -> u64 {
    mass_calculator().calc_non_contextual_masses(tx).transient_mass
}

fn prepare_bench_tx(tx: Transaction, entries: Vec<UtxoEntry>) -> BenchTx {
    BenchTx { tx: MutableTransaction::with_entries(tx, entries) }
}

fn input_outpoint(index: u32, nonce: u32) -> TransactionOutpoint {
    TransactionOutpoint {
        transaction_id: TransactionId::from_bytes([
            (nonce & 0xff) as u8,
            ((nonce >> 8) & 0xff) as u8,
            ((nonce >> 16) & 0xff) as u8,
            ((nonce >> 24) & 0xff) as u8,
            index as u8,
            0xaa,
            0xbb,
            0xcc,
            0xdd,
            0xee,
            0xf0,
            0x11,
            0x22,
            0x33,
            0x44,
            0x55,
            0x66,
            0x77,
            0x88,
            0x99,
            0x10,
            0x20,
            0x30,
            0x40,
            0x50,
            0x60,
            0x70,
            0x80,
            0x90,
            0xa0,
            0xb0,
            0xc0,
        ]),
        index,
    }
}

fn schnorr_keypair(seed: u32) -> Keypair {
    let secp = Secp256k1::new();
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&seed.to_le_bytes());
    bytes[4] = 1;
    let secret = SecretKey::from_slice(&bytes).expect("valid schnorr secret");
    Keypair::from_secret_key(&secp, &secret)
}

fn build_schnorr_2in1_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let kp1 = schnorr_keypair(nonce.wrapping_mul(2).wrapping_add(1));
    let kp2 = schnorr_keypair(nonce.wrapping_mul(2).wrapping_add(2));
    let out_kp = schnorr_keypair(nonce.wrapping_mul(2).wrapping_add(3));
    let dummy_out1 = input_outpoint(0, nonce.wrapping_mul(2));
    let dummy_out2 = input_outpoint(1, nonce.wrapping_mul(2).wrapping_add(1));

    let addr1 = Address::new(Prefix::Mainnet, Version::PubKey, &kp1.x_only_public_key().0.serialize());
    let addr2 = Address::new(Prefix::Mainnet, Version::PubKey, &kp2.x_only_public_key().0.serialize());
    let out_addr = Address::new(Prefix::Mainnet, Version::PubKey, &out_kp.x_only_public_key().0.serialize());

    let utxos = vec![
        UtxoEntry::new(20_000, pay_to_address_script(&addr1), 0, false, None),
        UtxoEntry::new(20_000, pay_to_address_script(&addr2), 0, false, None),
    ];
    let outputs = vec![TransactionOutput { value: 30_000, script_public_key: pay_to_address_script(&out_addr), covenant: None }];
    let mut tx = Transaction::new(
        1,
        vec![
            TransactionInput { previous_outpoint: dummy_out1, signature_script: vec![], sequence: 0, mass: ComputeBudget(10).into() },
            TransactionInput { previous_outpoint: dummy_out2, signature_script: vec![], sequence: 0, mass: ComputeBudget(10).into() },
        ],
        outputs,
        0,
        Default::default(),
        0,
        vec![],
    );

    for (input_idx, kp) in [kp1, kp2].iter().enumerate() {
        let populated = PopulatedTransaction::new(&tx, utxos.clone());
        let sig_hash = calc_schnorr_signature_hash(&populated, input_idx, SIG_HASH_ALL, &SigHashReusedValuesUnsync::new());
        let msg = Message::from_digest_slice(sig_hash.as_bytes().as_slice()).expect("valid sighash");
        let sig = kp.sign_schnorr(msg);
        tx.inputs[input_idx].signature_script = std::iter::once(65u8).chain(*sig.as_ref()).chain([SIG_HASH_ALL.to_u8()]).collect();
    }

    (tx, utxos)
}

fn build_op_dup_script_public_key_with_seed(seed: &[u8], dup_count: usize) -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    builder.add_data(seed).unwrap();
    for _ in 0..dup_count {
        builder.add_op(OpDup).unwrap();
    }
    for _ in 0..dup_count {
        builder.add_op(OpDrop).unwrap();
    }
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_op_dup_script_public_key() -> ScriptPublicKey {
    build_op_dup_script_public_key_with_seed(&[1u8], OP_DUP_BASE_DUP_COUNT)
}

fn build_op_dup_free_budget_script_public_key() -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..OP_DUP_FREE_BUDGET_DUP_COUNT {
        builder.add_op(OpDup).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_adversarial_output_script_public_key() -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    builder.add_data(&[0x5au8; ADVERSARIAL_OUTPUT_SPK_DATA_LEN]).unwrap();
    builder.add_op(OpDrop).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_introspection_cat_substr_math_script_public_key() -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();

    builder.add_op(OpDrop).unwrap();

    builder
        .add_i64(0)
        .unwrap()
        .add_op(codes::OpTxInputAmount)
        .unwrap()
        .add_i64(0)
        .unwrap()
        .add_op(codes::OpTxOutputAmount)
        .unwrap()
        .add_op(codes::OpSub)
        .unwrap()
        .add_i64(10)
        .unwrap()
        .add_op(codes::OpDiv)
        .unwrap()
        .add_i64(7)
        .unwrap()
        .add_op(codes::OpMod)
        .unwrap()
        .add_i64(6)
        .unwrap()
        .add_op(codes::OpNumEqualVerify)
        .unwrap();

    builder
        .add_i64(0)
        .unwrap()
        .add_op(codes::OpTxInputScriptSigLen)
        .unwrap()
        .add_i64(0)
        .unwrap()
        .add_op(codes::OpTxInputSpkLen)
        .unwrap()
        .add_op(codes::OpAdd)
        .unwrap()
        .add_i64(0)
        .unwrap()
        .add_op(codes::OpTxOutputSpkLen)
        .unwrap()
        .add_op(codes::OpAdd)
        .unwrap()
        .add_op(codes::OpTxPayloadLen)
        .unwrap()
        .add_op(codes::OpAdd)
        .unwrap()
        .add_i64(600)
        .unwrap()
        .add_op(codes::OpGreaterThan)
        .unwrap()
        .add_op(codes::OpVerify)
        .unwrap();

    for _ in 0..ADVERSARIAL_ROUNDS {
        builder
            .add_i64(0)
            .unwrap()
            .add_i64(ADVERSARIAL_SLICE_LEN)
            .unwrap()
            .add_op(codes::OpTxPayloadSubstr)
            .unwrap()
            .add_i64(0)
            .unwrap()
            .add_i64(0)
            .unwrap()
            .add_i64(ADVERSARIAL_SLICE_LEN)
            .unwrap()
            .add_op(codes::OpTxInputScriptSigSubstr)
            .unwrap()
            .add_op(codes::OpCat)
            .unwrap()
            .add_i64(0)
            .unwrap()
            .add_i64(0)
            .unwrap()
            .add_i64(ADVERSARIAL_SLICE_LEN)
            .unwrap()
            .add_op(codes::OpTxInputSpkSubstr)
            .unwrap()
            .add_op(codes::OpCat)
            .unwrap()
            .add_i64(0)
            .unwrap()
            .add_i64(0)
            .unwrap()
            .add_i64(ADVERSARIAL_SLICE_LEN)
            .unwrap()
            .add_op(codes::OpTxOutputSpkSubstr)
            .unwrap()
            .add_op(codes::OpCat)
            .unwrap()
            .add_i64(16)
            .unwrap()
            .add_i64(240)
            .unwrap()
            .add_op(codes::OpSubstr)
            .unwrap()
            .add_i64(8)
            .unwrap()
            .add_i64(200)
            .unwrap()
            .add_op(codes::OpSubstr)
            .unwrap()
            .add_i64(32)
            .unwrap()
            .add_i64(160)
            .unwrap()
            .add_op(codes::OpSubstr)
            .unwrap()
            .add_op(codes::OpSize)
            .unwrap()
            .add_i64(4)
            .unwrap()
            .add_op(codes::OpDiv)
            .unwrap()
            .add_i64(2)
            .unwrap()
            .add_op(codes::OpMul)
            .unwrap()
            .add_i64(1)
            .unwrap()
            .add_op(codes::OpAdd)
            .unwrap()
            .add_i64(65)
            .unwrap()
            .add_op(codes::OpNumEqualVerify)
            .unwrap()
            .add_op(codes::OpDrop)
            .unwrap();
    }

    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_blake2b_storm_script_public_key(rounds: usize) -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..rounds {
        builder.add_op(OpDup).unwrap();
        builder.add_op(codes::OpBlake2b).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_blake2b_with_key_storm_script_public_key(rounds: usize) -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..rounds {
        builder.add_op(codes::Op2Dup).unwrap();
        builder.add_op(codes::OpBlake2bWithKey).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDrop).unwrap();
    builder.add_op(OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_blake3_storm_script_public_key(rounds: usize) -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..rounds {
        builder.add_op(OpDup).unwrap();
        builder.add_op(codes::OpBlake3).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_blake3_with_key_storm_script_public_key(rounds: usize) -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..rounds {
        builder.add_op(codes::Op2Dup).unwrap();
        builder.add_op(codes::OpBlake3WithKey).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDrop).unwrap();
    builder.add_op(OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_sha256_storm_script_public_key(rounds: usize) -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..rounds {
        builder.add_op(OpDup).unwrap();
        builder.add_op(codes::OpSHA256).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_large_push_dup_cat_script_public_key() -> ScriptPublicKey {
    let mut builder = ScriptBuilder::new();
    for _ in 0..1 {
        builder.add_op(OpDup).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDup).unwrap();
    builder.add_op(codes::OpCat).unwrap();
    for _ in 0..1 {
        builder.add_op(OpDup).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDup).unwrap();
    builder.add_op(codes::OpCat).unwrap();
    builder.add_op(OpDup).unwrap();
    builder.add_op(codes::OpCat).unwrap();
    for _ in 0..12 {
        builder.add_op(OpDup).unwrap();
        builder.add_op(codes::OpBlake2b).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    for _ in 0..22 {
        builder.add_op(OpDup).unwrap();
    }
    for _ in 0..22 {
        builder.add_op(OpDrop).unwrap();
    }
    builder.add_op(OpDrop).unwrap();
    builder.add_op(codes::OpTrue).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn try_build_budgeted_single_input_tx(
    nonce: u32,
    input_spk: ScriptPublicKey,
    signature_script: Vec<u8>,
) -> Result<(Transaction, Vec<UtxoEntry>), String> {
    let outpoint = input_outpoint(0, nonce);
    let utxos = vec![UtxoEntry::new(20_000, input_spk, 0, false, None)];
    let mut tx = Transaction::new(
        1,
        vec![TransactionInput {
            previous_outpoint: outpoint,
            signature_script,
            sequence: 0,
            mass: TxInputMass::ComputeBudget(0.into()),
        }],
        vec![],
        0,
        Default::default(),
        0,
        vec![],
    );

    let reused_values = SigHashReusedValuesUnsync::new();
    let sig_cache = Cache::new(1);
    let populated = PopulatedTransaction::new(&tx, utxos.clone());
    let mut vm = TxScriptEngine::from_transaction_input_with_script_units_limit(
        &populated,
        &tx.inputs[0],
        0,
        &utxos[0],
        EngineCtx::new(&sig_cache).with_reused(&reused_values),
        pricing_flags(true),
        ScriptUnits(u64::MAX),
    );
    vm.execute().map_err(|err| format!("failed to measure input #0: {err}"))?;
    let compute_budget = ComputeBudget::checked_covering_script_units(vm.used_script_units())
        .ok_or_else(|| "required compute budget does not fit for input #0".to_string())?;
    tx.inputs[0].mass = compute_budget.into();

    Ok((tx, utxos))
}

fn build_budgeted_single_input_tx(nonce: u32, input_spk: ScriptPublicKey, signature_script: Vec<u8>) -> (Transaction, Vec<UtxoEntry>) {
    try_build_budgeted_single_input_tx(nonce, input_spk, signature_script).unwrap_or_else(|err| panic!("{err}"))
}

fn build_op_dup_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_budgeted_single_input_tx(nonce, build_op_dup_script_public_key(), vec![])
}

fn build_op_dup_free_budget_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let redeem_script = build_op_dup_free_budget_script_public_key();
    let signature_prefix = ScriptBuilder::new().add_data(&vec![0x5au8; op_dup_free_budget_element_len()]).unwrap().drain();
    let signature_script = pay_to_script_hash_signature_script(redeem_script.script().to_vec(), signature_prefix).unwrap();
    build_budgeted_single_input_tx(nonce, pay_to_script_hash_script(redeem_script.script()), signature_script)
}

fn build_op_dup_p2sh_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let redeem_script = build_op_dup_script_public_key();
    let signature_script = pay_to_script_hash_signature_script(redeem_script.script().to_vec(), vec![]).unwrap();
    build_budgeted_single_input_tx(nonce, pay_to_script_hash_script(redeem_script.script()), signature_script)
}

fn build_introspection_cat_substr_math_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let outpoint = input_outpoint(0, nonce);
    let input_spk = build_introspection_cat_substr_math_script_public_key();
    let output_spk = build_adversarial_output_script_public_key();
    let signature_script = ScriptBuilder::new().add_data(&[0xa5u8; ADVERSARIAL_SIGSCRIPT_DATA_LEN]).unwrap().drain();
    let entries = vec![UtxoEntry::new(20_000, input_spk, 0, false, None)];
    let tx = Transaction::new(
        1,
        vec![TransactionInput {
            previous_outpoint: outpoint,
            signature_script,
            sequence: 0,
            mass: TxInputMass::ComputeBudget(0.into()),
        }],
        vec![TransactionOutput { value: 10_000, script_public_key: output_spk, covenant: None }],
        0,
        Default::default(),
        0,
        vec![0x3cu8; ADVERSARIAL_PAYLOAD_LEN],
    );
    (build_budgeted_charged_tx("introspection_cat_substr_math", &tx, &entries), entries)
}

fn build_blake2b_storm_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_blake2b_storm_tx_with_rounds(nonce, HASHING_ROUNDS)
}

fn build_blake2b_storm_tx_with_rounds(nonce: u32, rounds: usize) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_tx_with_rounds(nonce, rounds, "blake2b_storm", build_blake2b_storm_script_public_key, false)
}

fn build_blake2b_with_key_storm_tx_with_rounds(nonce: u32, rounds: usize) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_tx_with_rounds(nonce, rounds, "blake2b_with_key_storm", build_blake2b_with_key_storm_script_public_key, true)
}

fn build_blake3_storm_tx_with_rounds(nonce: u32, rounds: usize) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_tx_with_rounds(nonce, rounds, "blake3_storm", build_blake3_storm_script_public_key, false)
}

fn build_blake3_with_key_storm_tx_with_rounds(nonce: u32, rounds: usize) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_tx_with_rounds(nonce, rounds, "blake3_with_key_storm", build_blake3_with_key_storm_script_public_key, true)
}

fn build_sha256_storm_tx_with_rounds(nonce: u32, rounds: usize) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_tx_with_rounds(nonce, rounds, "sha256_storm", build_sha256_storm_script_public_key, false)
}

fn build_hash_storm_tx_with_rounds(
    nonce: u32,
    rounds: usize,
    label: &str,
    script_builder: fn(usize) -> ScriptPublicKey,
    keyed: bool,
) -> (Transaction, Vec<UtxoEntry>) {
    let outpoint = input_outpoint(0, nonce);
    let redeem_script = script_builder(rounds);
    let mut signature_builder = ScriptBuilder::new();
    signature_builder.add_data(&vec![0x42u8; HASHING_SIGSCRIPT_DATA_LEN]).unwrap();
    if keyed {
        signature_builder.add_data(&[0x24u8; HASHING_KEY_LEN]).unwrap();
    }
    let signature_prefix = signature_builder.drain();
    let signature_script = pay_to_script_hash_signature_script(redeem_script.script().to_vec(), signature_prefix).unwrap();
    let entries = vec![UtxoEntry::new(20_000, pay_to_script_hash_script(redeem_script.script()), 0, false, None)];
    let tx = Transaction::new(
        1,
        vec![TransactionInput {
            previous_outpoint: outpoint,
            signature_script,
            sequence: 0,
            mass: TxInputMass::ComputeBudget(0.into()),
        }],
        vec![],
        0,
        Default::default(),
        0,
        vec![],
    );
    (build_budgeted_charged_tx(label, &tx, &entries), entries)
}

fn fits_block_mass(tx: &Transaction) -> bool {
    compute_mass(tx) <= BLOCK_COMPUTE_MASS_LIMIT && transient_mass(tx) <= BLOCK_TRANSIENT_MASS_LIMIT
}

fn build_blake2b_storm_single_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_single_tx(nonce, build_blake2b_storm_tx_with_rounds)
}

fn build_blake2b_with_key_storm_single_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_single_tx(nonce, build_blake2b_with_key_storm_tx_with_rounds)
}

fn build_blake3_storm_single_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_single_tx(nonce, build_blake3_storm_tx_with_rounds)
}

fn build_blake3_with_key_storm_single_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_single_tx(nonce, build_blake3_with_key_storm_tx_with_rounds)
}

fn build_sha256_storm_single_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_hash_storm_single_tx(nonce, build_sha256_storm_tx_with_rounds)
}

fn build_hash_storm_single_tx(nonce: u32, builder: RoundTxBuilder) -> (Transaction, Vec<UtxoEntry>) {
    let mut low_rounds = HASHING_ROUNDS;
    let mut high_rounds = low_rounds;
    let mut best = builder(nonce, low_rounds);

    loop {
        high_rounds = high_rounds.saturating_mul(2);
        let candidate = builder(nonce, high_rounds);
        if !fits_block_mass(&candidate.0) {
            break;
        }
        low_rounds = high_rounds;
        best = candidate;
    }

    while low_rounds + 1 < high_rounds {
        let mid_rounds = low_rounds + (high_rounds - low_rounds) / 2;
        let candidate = builder(nonce, mid_rounds);
        if fits_block_mass(&candidate.0) {
            low_rounds = mid_rounds;
            best = candidate;
        } else {
            high_rounds = mid_rounds;
        }
    }

    best
}

fn build_single_stark_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_budgeted_single_input_tx(nonce, ScriptPublicKey::new(0, build_stark_script(false).into()), vec![])
}

fn build_large_push_dup_cat_tx_with_data_len(nonce: u32, data_len: usize) -> (Transaction, Vec<UtxoEntry>) {
    let signature_script = ScriptBuilder::new().add_data(&vec![0x6du8; data_len]).unwrap().drain();
    build_budgeted_single_input_tx(nonce, build_large_push_dup_cat_script_public_key(), signature_script)
}

fn build_large_push_dup_cat_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let mut low_len = 0usize;
    let mut high_len = LARGE_PUSH_DUP_CAT_DATA_LEN_UPPER_BOUND.saturating_add(1);
    let mut best = build_large_push_dup_cat_tx_with_data_len(nonce, low_len);

    while low_len + 1 < high_len {
        let mid_len = low_len + (high_len - low_len) / 2;
        let candidate = build_large_push_dup_cat_tx_with_data_len(nonce, mid_len);
        if fits_block_mass(&candidate.0) {
            low_len = mid_len;
            best = candidate;
        } else {
            high_len = mid_len;
        }
    }

    best
}

fn build_op_dup_one_tx_script_public_key(nonce: u32) -> ScriptPublicKey {
    let mut script = ScriptBuilder::new();
    script.add_i64(1).unwrap();
    for _ in 0..OP_DUP_BASE_DUP_COUNT {
        script.add_op(OpDup).unwrap();
    }
    for _ in 0..OP_DUP_BASE_DUP_COUNT {
        script.add_op(OpDrop).unwrap();
    }

    let mut best_script = ScriptPublicKey::new(0, script.drain().into());
    let mut pair_count = 0usize;
    loop {
        let mut candidate_builder = ScriptBuilder::new();
        candidate_builder.add_i64(1).unwrap();
        for _ in 0..OP_DUP_BASE_DUP_COUNT {
            candidate_builder.add_op(OpDup).unwrap();
        }
        for _ in 0..pair_count + OP_DUP_ONE_TX_PAIR_SEARCH_STEP {
            candidate_builder.add_op(OpDrop).unwrap();
            candidate_builder.add_op(OpDup).unwrap();
        }
        for _ in 0..OP_DUP_BASE_DUP_COUNT {
            candidate_builder.add_op(OpDrop).unwrap();
        }
        let candidate_script = ScriptPublicKey::new(0, candidate_builder.drain().into());
        let Ok((candidate_tx, _)) = try_build_budgeted_single_input_tx(nonce, candidate_script.clone(), vec![]) else {
            break;
        };
        if compute_mass(&candidate_tx) > BLOCK_COMPUTE_MASS_LIMIT {
            break;
        }
        best_script = candidate_script;
        pair_count += OP_DUP_ONE_TX_PAIR_SEARCH_STEP;
    }

    best_script
}

fn build_op_dup_one_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_budgeted_single_input_tx(nonce, build_op_dup_one_tx_script_public_key(nonce), vec![])
}

fn build_budgeted_charged_tx(label: &str, tx: &Transaction, entries: &[UtxoEntry]) -> Transaction {
    let reused_values = SigHashReusedValuesUnsync::new();
    let sig_cache = Cache::new(10_000);
    let flags = pricing_flags(true);
    let populated = PopulatedTransaction::new(tx, entries.to_vec());
    let mut budgeted_tx = tx.clone();

    for (input_idx, input) in tx.inputs.iter().enumerate() {
        let utxo = populated.utxo(input_idx).expect("input utxo");
        let mut vm = TxScriptEngine::from_transaction_input_with_script_units_limit(
            &populated,
            input,
            input_idx,
            utxo,
            EngineCtx::new(&sig_cache).with_reused(&reused_values),
            flags,
            ScriptUnits(u64::MAX),
        );
        vm.execute().unwrap_or_else(|err| panic!("failed to measure {label} input #{input_idx}: {err}"));
        let compute_budget = ComputeBudget::checked_covering_script_units(vm.used_script_units())
            .unwrap_or_else(|| panic!("required compute budget does not fit for {label} input #{input_idx}"));
        budgeted_tx.inputs[input_idx].mass = compute_budget.into();
    }

    budgeted_tx
}

fn pack_repeated_txs(name: &'static str, builder: TxBuilder) -> BenchBlock {
    let mut txs = Vec::new();
    let mut total_mass = 0u64;
    let mut total_transient_mass = 0u64;
    let mut total_inputs = 0usize;
    let mut nonce = 0u32;

    loop {
        let (tx, entries) = builder(nonce);
        let tx_mass = compute_mass(&tx);
        let tx_transient_mass = transient_mass(&tx);
        if total_mass + tx_mass > BLOCK_COMPUTE_MASS_LIMIT || total_transient_mass + tx_transient_mass > BLOCK_TRANSIENT_MASS_LIMIT {
            break;
        }
        total_inputs += tx.inputs.len();
        total_mass += tx_mass;
        total_transient_mass += tx_transient_mass;
        txs.push(prepare_bench_tx(tx, entries));
        nonce = nonce.wrapping_add(1);
    }

    BenchBlock {
        name,
        tx_count: txs.len(),
        input_count: total_inputs,
        compute_mass: total_mass,
        transient_mass: total_transient_mass,
        txs,
    }
}

fn single_tx_block(name: &'static str, tx: Transaction, entries: Vec<UtxoEntry>) -> BenchBlock {
    BenchBlock {
        name,
        tx_count: 1,
        input_count: tx.inputs.len(),
        compute_mass: compute_mass(&tx),
        transient_mass: transient_mass(&tx),
        txs: vec![prepare_bench_tx(tx, entries)],
    }
}

fn build_schnorr_block() -> BenchBlock {
    pack_repeated_txs("schnorr_2in1", build_schnorr_2in1_tx)
}

fn build_op_dup_block() -> BenchBlock {
    pack_repeated_txs("op_dup_243", build_op_dup_tx)
}

fn build_op_dup_free_budget_block() -> BenchBlock {
    pack_repeated_txs("op_dup_9b_free_budget", build_op_dup_free_budget_tx)
}

fn build_op_dup_p2sh_block() -> BenchBlock {
    pack_repeated_txs("op_dup_243_p2sh", build_op_dup_p2sh_tx)
}

fn build_introspection_cat_substr_math_block() -> BenchBlock {
    pack_repeated_txs("introspection_cat_substr_math", build_introspection_cat_substr_math_tx)
}

fn build_blake2b_storm_block() -> BenchBlock {
    pack_repeated_txs("blake2b_storm", build_blake2b_storm_tx)
}

fn build_blake2b_storm_single_tx_block() -> BenchBlock {
    let (tx, entries) = build_blake2b_storm_single_tx(0);
    single_tx_block("blake2b_storm_single_tx", tx, entries)
}

fn build_blake2b_with_key_storm_single_tx_block() -> BenchBlock {
    let (tx, entries) = build_blake2b_with_key_storm_single_tx(0);
    single_tx_block("blake2b_with_key_storm_single_tx", tx, entries)
}

fn build_blake3_storm_single_tx_block() -> BenchBlock {
    let (tx, entries) = build_blake3_storm_single_tx(0);
    single_tx_block("blake3_storm_single_tx", tx, entries)
}

fn build_blake3_with_key_storm_single_tx_block() -> BenchBlock {
    let (tx, entries) = build_blake3_with_key_storm_single_tx(0);
    single_tx_block("blake3_with_key_storm_single_tx", tx, entries)
}

fn build_sha256_storm_single_tx_block() -> BenchBlock {
    let (tx, entries) = build_sha256_storm_single_tx(0);
    single_tx_block("sha256_storm_single_tx", tx, entries)
}

fn build_large_push_dup_cat_block() -> BenchBlock {
    let (tx, entries) = build_large_push_dup_cat_tx(0);
    let tx_compute_mass = compute_mass(&tx);
    let tx_transient_mass = transient_mass(&tx);
    assert!(
        tx_compute_mass <= BLOCK_COMPUTE_MASS_LIMIT,
        "large_push_dup_cat compute mass {tx_compute_mass} exceeds block limit {BLOCK_COMPUTE_MASS_LIMIT}"
    );
    assert!(
        tx_transient_mass <= BLOCK_TRANSIENT_MASS_LIMIT,
        "large_push_dup_cat transient mass {tx_transient_mass} exceeds block limit {BLOCK_TRANSIENT_MASS_LIMIT}"
    );
    single_tx_block("large_push_dup_cat", tx, entries)
}

fn build_op_dup_one_tx_block() -> BenchBlock {
    let (tx, entries) = build_op_dup_one_tx(0);
    single_tx_block("op_dup_one_tx", tx, entries)
}

fn build_single_stark_block() -> BenchBlock {
    let (tx, entries) = build_single_stark_tx(0);
    single_tx_block("single_stark", tx, entries)
}

fn bench_blocks() -> &'static [BenchBlock] {
    static BLOCKS: OnceLock<Vec<BenchBlock>> = OnceLock::new();
    BLOCKS.get_or_init(|| {
        let stack_entry_inline_capacity = stack_entry_inline_capacity();
        let op_dup_free_budget_element_len = op_dup_free_budget_element_len();

        assert_eq!(
            stack_entry_inline_capacity, 8,
            "txscript stack entry inline capacity changed; revisit op_dup_9b_free_budget assumptions"
        );
        assert_eq!(
            op_dup_free_budget_element_len, 9,
            "op_dup_9b_free_budget should stay one byte above the stack-entry inline boundary"
        );

        let blocks = vec![
            build_schnorr_block(),
            build_introspection_cat_substr_math_block(),
            build_blake2b_storm_block(),
            build_blake2b_storm_single_tx_block(),
            build_blake2b_with_key_storm_single_tx_block(),
            build_blake3_storm_single_tx_block(),
            build_blake3_with_key_storm_single_tx_block(),
            build_sha256_storm_single_tx_block(),
            build_large_push_dup_cat_block(),
            build_op_dup_block(),
            build_op_dup_free_budget_block(),
            build_op_dup_p2sh_block(),
            build_op_dup_one_tx_block(),
            build_single_stark_block(),
        ];

        for block in &blocks {
            eprintln!(
                "bench block {}: {} txs, {} inputs, compute mass {}, transient mass {}, {}",
                block.name,
                block.tx_count,
                block.input_count,
                block.compute_mass,
                block.transient_mass,
                format_average_input_budget(block)
            );
        }
        blocks
    })
}

fn validate_block_sequential(block: &BenchBlock) {
    let cache = Cache::new(block.input_count as u64);
    let flags = pricing_flags(true);

    for bench_tx in &block.txs {
        let verifiable = bench_tx.tx.as_verifiable();
        let reused_values = SigHashReusedValuesUnsync::new();
        let ctx = EngineCtx::new(&cache).with_reused(&reused_values);

        for (input_idx, (input, utxo)) in verifiable.populated_inputs().enumerate() {
            let script_units_limit = input.mass.allowed_script_units();
            let mut vm = TxScriptEngine::from_transaction_input_with_script_units_limit(
                &verifiable,
                input,
                input_idx,
                utxo,
                ctx,
                flags,
                script_units_limit,
            );
            vm.execute().unwrap();
        }
    }
}

fn validate_block_parallel(block: &BenchBlock, pool: &rayon::ThreadPool) {
    let cache = Cache::new(block.input_count as u64);
    let flags = pricing_flags(true);

    pool.install(|| {
        block.txs.par_iter().try_for_each(|bench_tx| -> Result<(), TxScriptError> {
            let verifiable = bench_tx.tx.as_verifiable();
            let reused_values = SigHashReusedValuesSync::new();
            let ctx = EngineCtx::new(&cache).with_reused(&reused_values);

            (0..verifiable.inputs().len()).into_par_iter().try_for_each(|input_idx| {
                let (input, utxo) = verifiable.populated_input(input_idx);
                let script_units_limit = input.mass.allowed_script_units();
                let mut vm = TxScriptEngine::from_transaction_input_with_script_units_limit(
                    &verifiable,
                    input,
                    input_idx,
                    utxo,
                    ctx,
                    flags,
                    script_units_limit,
                );
                vm.execute()
            })
        })
    })
    .unwrap();
}

fn benchmark_pricing(c: &mut Criterion) {
    let mut group = c.benchmark_group("script_pricing_workloads");
    group.sampling_mode(SamplingMode::Flat);
    group.measurement_time(Duration::from_secs(15));

    for block in bench_blocks() {
        group.bench_with_input(BenchmarkId::new("single_thread", block.name), block, |b, block| {
            b.iter(|| validate_block_sequential(black_box(block)));
        });

        if block.tx_count == 1 && block.input_count == 1 {
            continue;
        }

        for threads in [2usize, 4, 8] {
            let pool = ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            group.bench_with_input(BenchmarkId::new(format!("rayon_threads_{threads}"), block.name), block, |b, block| {
                b.iter(|| validate_block_parallel(black_box(block), black_box(&pool)));
            });
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_output_color(true);
    targets = benchmark_pricing
}
criterion_main!(benches);
