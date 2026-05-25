use std::sync::OnceLock;
use std::time::Duration;

use ark_bn254::{Bn254, Fr};
use ark_groth16::{Groth16, VerifyingKey};
use ark_relations::gr1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_snark::SNARK;
use criterion::{BenchmarkId, Criterion, SamplingMode, black_box, criterion_group, criterion_main};
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::config::params::MAINNET_PARAMS;
use kaspa_consensus_core::hashing::sighash::{
    SigHashReusedValuesSync, SigHashReusedValuesUnsync, calc_ecdsa_signature_hash, calc_schnorr_signature_hash,
};
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
    EngineCtx, EngineFlags, MAX_STACK_SIZE, TxScriptEngine, max_script_element_size, pay_to_address_script, pay_to_script_hash_script,
    pay_to_script_hash_signature_script_with_flags,
    zk_precompiles::{
        tags::ZkTag,
        tests::helpers::{build_groth_script, load_groth_fields, load_stark_fields},
    },
};
use kaspa_txscript_errors::TxScriptError;
use rand::{SeedableRng, rngs::StdRng};
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
const LARGE_PUSH_DUP_CAT_CAT_COUNT: usize = 3;
const LARGE_PUSH_DUP_CAT_EXPANSION_FACTOR: usize = 1 << LARGE_PUSH_DUP_CAT_CAT_COUNT;
const LARGE_PUSH_DUP_CAT_DATA_LEN_UPPER_BOUND: usize = max_script_element_size(true) / LARGE_PUSH_DUP_CAT_EXPANSION_FACTOR;
const LARGE_PUSH_DUP_CAT_DUP_COUNT: usize = MAX_STACK_SIZE - 1;
const GROTH16_LARGE_VK_PADDING_CAT_COUNT: usize = 19;
const GROTH16_2X_COUNT: usize = 2;
const GROTH16_3X_COUNT: usize = 3;
const OP_DUP_BASE_DUP_COUNT: usize = 243;
const OP_DUP_FREE_BUDGET_DUP_COUNT: usize = 1107;
const OP_DUP_ONE_TX_PAIR_SEARCH_STEP: usize = 20;

struct Groth16PublicInputCircuit {
    public_input_count: usize,
    public_input: Fr,
}

impl ConstraintSynthesizer<Fr> for Groth16PublicInputCircuit {
    fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
        let public_input = self.public_input;
        let mut running_sum = Fr::from(0u64);
        let mut sum_var = cs.new_witness_variable(|| Ok(Fr::from(0u64)))?;
        for _ in 0..self.public_input_count {
            let input = cs.new_input_variable(|| Ok(public_input))?;
            running_sum += public_input;
            let new_sum_var = cs.new_witness_variable(|| Ok(running_sum))?;
            cs.enforce_r1cs_constraint(
                || ark_relations::lc!() + sum_var + input,
                || ark_relations::lc!() + ark_relations::gr1cs::Variable::One,
                || ark_relations::lc!() + new_sum_var,
            )?;
            sum_var = new_sum_var;
        }
        Ok(())
    }
}

struct Groth16SerializedFixture {
    vk_bytes: Vec<u8>,
    proof_bytes: Vec<u8>,
    public_input_bytes: Vec<u8>,
}

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
    allow_zk_failure: bool,
}

type TxBuilder = fn(u32) -> (Transaction, Vec<UtxoEntry>);
type RoundTxBuilder = fn(u32, usize) -> (Transaction, Vec<UtxoEntry>);

fn pricing_flags(covenants_enabled: bool) -> EngineFlags {
    EngineFlags { covenants_enabled, zk_hardening_enabled: covenants_enabled, sigop_script_units: Gram(1000).into() }
}

fn new_script_builder() -> ScriptBuilder {
    ScriptBuilder::with_flags(pricing_flags(true))
}

fn new_p2sh_signature_script(redeem_script: Vec<u8>, signature: Vec<u8>) -> Vec<u8> {
    pay_to_script_hash_signature_script_with_flags(redeem_script, signature, pricing_flags(true)).unwrap()
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

fn build_ecdsa_2in1_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let kp1 = schnorr_keypair(nonce.wrapping_mul(2).wrapping_add(1));
    let kp2 = schnorr_keypair(nonce.wrapping_mul(2).wrapping_add(2));
    let out_kp = schnorr_keypair(nonce.wrapping_mul(2).wrapping_add(3));
    let dummy_out1 = input_outpoint(0, nonce.wrapping_mul(2));
    let dummy_out2 = input_outpoint(1, nonce.wrapping_mul(2).wrapping_add(1));

    let addr1 = Address::new(Prefix::Mainnet, Version::PubKeyECDSA, &kp1.public_key().serialize());
    let addr2 = Address::new(Prefix::Mainnet, Version::PubKeyECDSA, &kp2.public_key().serialize());
    let out_addr = Address::new(Prefix::Mainnet, Version::PubKeyECDSA, &out_kp.public_key().serialize());

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
        let sig_hash = calc_ecdsa_signature_hash(&populated, input_idx, SIG_HASH_ALL, &SigHashReusedValuesUnsync::new());
        let msg = Message::from_digest_slice(sig_hash.as_bytes().as_slice()).expect("valid sighash");
        let sig = kp.secret_key().sign_ecdsa(msg);
        tx.inputs[input_idx].signature_script =
            std::iter::once(65u8).chain(sig.serialize_compact()).chain([SIG_HASH_ALL.to_u8()]).collect();
    }

    (tx, utxos)
}

fn build_op_dup_script_public_key_with_seed(seed: &[u8], dup_count: usize) -> ScriptPublicKey {
    let mut builder = new_script_builder();
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
    let mut builder = new_script_builder();
    for _ in 0..OP_DUP_FREE_BUDGET_DUP_COUNT {
        builder.add_op(OpDup).unwrap();
        builder.add_op(OpDrop).unwrap();
    }
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_adversarial_output_script_public_key() -> ScriptPublicKey {
    let mut builder = new_script_builder();
    builder.add_data(&[0x5au8; ADVERSARIAL_OUTPUT_SPK_DATA_LEN]).unwrap();
    builder.add_op(OpDrop).unwrap();
    ScriptPublicKey::new(0, builder.drain().into())
}

fn build_introspection_cat_substr_math_script_public_key() -> ScriptPublicKey {
    let mut builder = new_script_builder();

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
    let mut builder = new_script_builder();
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
    let mut builder = new_script_builder();
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
    let mut builder = new_script_builder();
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
    let mut builder = new_script_builder();
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
    let mut builder = new_script_builder();
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
    let mut builder = new_script_builder();
    for _ in 0..LARGE_PUSH_DUP_CAT_CAT_COUNT {
        builder.add_op(OpDup).unwrap();
        builder.add_op(codes::OpCat).unwrap();
    }
    for _ in 0..LARGE_PUSH_DUP_CAT_DUP_COUNT {
        builder.add_op(OpDup).unwrap();
    }
    for _ in 0..LARGE_PUSH_DUP_CAT_DUP_COUNT {
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
    try_build_budgeted_single_input_tx_with_error_filter(nonce, input_spk, signature_script, |_| false)
}

fn try_build_budgeted_single_input_tx_with_error_filter<F>(
    nonce: u32,
    input_spk: ScriptPublicKey,
    signature_script: Vec<u8>,
    accept_error: F,
) -> Result<(Transaction, Vec<UtxoEntry>), String>
where
    F: Fn(&TxScriptError) -> bool,
{
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
    match vm.execute() {
        Ok(()) => {}
        Err(err) if accept_error(&err) => {}
        Err(err) => return Err(format!("failed to measure input #0: {err}")),
    }
    let compute_budget = ComputeBudget::checked_covering_script_units(vm.used_script_units())
        .ok_or_else(|| "required compute budget does not fit for input #0".to_string())?;
    tx.inputs[0].mass = compute_budget.into();

    Ok((tx, utxos))
}

fn try_build_budgeted_single_input_tx_allowing_zk_failure(
    nonce: u32,
    input_spk: ScriptPublicKey,
    signature_script: Vec<u8>,
) -> Result<(Transaction, Vec<UtxoEntry>), String> {
    try_build_budgeted_single_input_tx_with_error_filter(nonce, input_spk, signature_script, |err| {
        matches!(err, TxScriptError::ZkIntegrity(_))
    })
}

fn build_single_input_tx_with_compute_budget(
    nonce: u32,
    input_spk: ScriptPublicKey,
    signature_script: Vec<u8>,
    compute_budget: ComputeBudget,
) -> (Transaction, Vec<UtxoEntry>) {
    let outpoint = input_outpoint(0, nonce);
    let utxos = vec![UtxoEntry::new(20_000, input_spk, 0, false, None)];
    let tx = Transaction::new(
        1,
        vec![TransactionInput {
            previous_outpoint: outpoint,
            signature_script,
            sequence: 0,
            mass: TxInputMass::ComputeBudget(compute_budget),
        }],
        vec![],
        0,
        Default::default(),
        0,
        vec![],
    );
    (tx, utxos)
}

fn build_budgeted_single_input_tx(nonce: u32, input_spk: ScriptPublicKey, signature_script: Vec<u8>) -> (Transaction, Vec<UtxoEntry>) {
    try_build_budgeted_single_input_tx(nonce, input_spk, signature_script).unwrap_or_else(|err| panic!("{err}"))
}

fn build_op_dup_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_budgeted_single_input_tx(nonce, build_op_dup_script_public_key(), vec![])
}

fn build_op_dup_free_budget_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let redeem_script = build_op_dup_free_budget_script_public_key();
    let signature_prefix = new_script_builder().add_data(&vec![0x5au8; op_dup_free_budget_element_len()]).unwrap().drain();
    let signature_script = new_p2sh_signature_script(redeem_script.script().to_vec(), signature_prefix);
    build_budgeted_single_input_tx(nonce, pay_to_script_hash_script(redeem_script.script()), signature_script)
}

fn build_op_dup_p2sh_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let redeem_script = build_op_dup_script_public_key();
    let signature_script = new_p2sh_signature_script(redeem_script.script().to_vec(), vec![]);
    build_budgeted_single_input_tx(nonce, pay_to_script_hash_script(redeem_script.script()), signature_script)
}

fn build_introspection_cat_substr_math_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let outpoint = input_outpoint(0, nonce);
    let input_spk = build_introspection_cat_substr_math_script_public_key();
    let output_spk = build_adversarial_output_script_public_key();
    let signature_script = new_script_builder().add_data(&[0xa5u8; ADVERSARIAL_SIGSCRIPT_DATA_LEN]).unwrap().drain();
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
    let mut signature_builder = new_script_builder();
    signature_builder.add_data(&vec![0x42u8; HASHING_SIGSCRIPT_DATA_LEN]).unwrap();
    if keyed {
        signature_builder.add_data(&[0x24u8; HASHING_KEY_LEN]).unwrap();
    }
    let signature_prefix = signature_builder.drain();
    let signature_script = new_p2sh_signature_script(redeem_script.script().to_vec(), signature_prefix);
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
    let redeem_script = build_stark_p2sh_redeem_script();
    let script_public_key = pay_to_script_hash_script(&redeem_script);
    let signature_script = build_stark_p2sh_signature_script(redeem_script);
    build_budgeted_single_input_tx(nonce, script_public_key, signature_script)
}

fn build_stark_p2sh_signature_script(redeem_script: Vec<u8>) -> Vec<u8> {
    let (_, seal, claim, _, control_index, control_digests, journal, _) = load_stark_fields();
    let mut signature_prefix = new_script_builder();
    signature_prefix
        .add_data(&claim)
        .unwrap()
        .add_data(&control_index)
        .unwrap()
        .add_data(&control_digests)
        .unwrap()
        .add_data(&seal)
        .unwrap()
        .add_data(&journal)
        .unwrap();
    new_p2sh_signature_script(redeem_script, signature_prefix.drain())
}

fn build_stark_p2sh_redeem_script() -> Vec<u8> {
    let (control_id, _, _, hashfn, _, _, _, image_id) = load_stark_fields();
    let stark_tag = ZkTag::R0Succinct as u8;
    let mut builder = new_script_builder();
    builder
        .add_data(&image_id)
        .unwrap()
        .add_data(&control_id)
        .unwrap()
        .add_data(&hashfn)
        .unwrap()
        .add_data(&[stark_tag])
        .unwrap()
        .add_op(codes::OpZkPrecompile)
        .unwrap();
    builder.drain()
}

fn build_groth16_repeated_script(call_count: usize) -> Vec<u8> {
    assert!(call_count > 0, "groth16 script should contain at least one call");

    let groth16_script = build_groth_script();
    let mut script = Vec::with_capacity(groth16_script.len() * call_count + call_count.saturating_sub(1));
    for call_idx in 0..call_count {
        if call_idx > 0 {
            script.push(OpDrop);
        }
        script.extend_from_slice(&groth16_script);
    }
    script
}

fn build_groth16_repeated_tx(nonce: u32, call_count: usize) -> (Transaction, Vec<UtxoEntry>) {
    build_budgeted_single_input_tx(nonce, ScriptPublicKey::new(0, build_groth16_repeated_script(call_count).into()), vec![])
}

fn build_groth16_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_groth16_repeated_tx(nonce, 1)
}

fn build_groth16_3x_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    static SCRIPT: OnceLock<Vec<u8>> = OnceLock::new();
    let script = SCRIPT
        .get_or_init(|| {
            let public_input_count = groth16_3x_pub_input_count();
            build_groth16_repeated_valid_script(public_input_count, GROTH16_3X_COUNT)
                .expect("cached groth16 3x public input count should be valid")
        })
        .clone();
    let candidate = try_build_budgeted_single_input_tx(nonce, ScriptPublicKey::new(0, script.into()), vec![])
        .expect("cached groth16 3x script should be valid");
    assert!(fits_block_mass(&candidate.0), "cached groth16 3x public input count should stay within block mass limits");
    candidate
}

// Duplicates a high-Hamming public input and extends gamma_abc_g1 to stress
// Ark's Groth16 prepare_inputs path. The proof may fail after the prepared-input
// work; this bench is pricing the input-preparation cost.
fn build_groth16_prepare_inputs_script(public_input_count: usize, vk_gamma_abc_count: usize) -> Result<Vec<u8>, String> {
    assert!(public_input_count > 0, "groth16 prepare-inputs script should contain public inputs");
    assert!(
        vk_gamma_abc_count > public_input_count,
        "groth16 verifying key should contain one gamma_abc_g1 base point plus all public inputs"
    );

    let (vk_bytes, proof_bytes, _) = load_groth_fields();
    let mut input = Vec::new();
    groth16_high_hamming_public_input()
        .serialize_uncompressed(&mut input)
        .map_err(|err| format!("failed to serialize groth16 public input: {err}"))?;
    let (extended_vk_bytes, _) = groth16_extended_vk_bytes(vk_bytes.as_slice(), vk_gamma_abc_count)?;

    build_groth16_repeated_input_script(public_input_count, &extended_vk_bytes, &proof_bytes, &input)
}

fn build_groth16_repeated_input_script(
    public_input_count: usize,
    vk_bytes: &[u8],
    proof_bytes: &[u8],
    input: &[u8],
) -> Result<Vec<u8>, String> {
    assert!(public_input_count > 0, "groth16 repeated-input script should contain public inputs");

    let mut builder = new_script_builder();
    builder.add_data(input).map_err(|err| format!("failed to add groth16 public input: {err}"))?;
    for _ in 1..public_input_count {
        builder.add_op(OpDup).map_err(|err| format!("failed to duplicate groth16 public input: {err}"))?;
    }
    builder
        .add_i64(public_input_count as i64)
        .map_err(|err| format!("failed to add groth16 public input count: {err}"))?
        .add_data(proof_bytes)
        .map_err(|err| format!("failed to add groth16 proof: {err}"))?
        .add_data(vk_bytes)
        .map_err(|err| format!("failed to add groth16 verifying key: {err}"))?
        .add_data(&[ZkTag::Groth16 as u8])
        .map_err(|err| format!("failed to add groth16 tag: {err}"))?
        .add_op(codes::OpZkPrecompile)
        .map_err(|err| format!("failed to add groth16 opcode: {err}"))?;

    Ok(builder.drain())
}

fn build_repeated_groth16_script(groth16_script: &[u8], count: usize) -> Vec<u8> {
    assert!(count > 0, "repeated groth16 script should contain at least one call");

    let mut script = Vec::with_capacity(groth16_script.len() * count + count.saturating_sub(1));
    for call_idx in 0..count {
        if call_idx > 0 {
            script.push(OpDrop);
        }
        script.extend_from_slice(groth16_script);
    }
    script
}

fn build_groth16_prepare_inputs_repeated_script(public_input_count: usize, call_count: usize) -> Result<Vec<u8>, String> {
    let groth16_script = build_groth16_prepare_inputs_script(public_input_count, public_input_count + 1)?;
    Ok(build_repeated_groth16_script(&groth16_script, call_count))
}

fn groth16_high_hamming_public_input() -> Fr {
    // Ark Groth16 prepares each public input with affine scalar multiplication.
    // Its current BN254 path uses double-and-add, so the scalar bit length and
    // Hamming weight affect the cost. Use 2^253 - 1 to cover the worst case of that path.
    let mut bytes = [0xffu8; 32];
    bytes[31] = 0x1f;
    Fr::deserialize_uncompressed(bytes.as_slice()).expect("2^253 - 1 should fit in BN254 Fr")
}

fn build_groth16_valid_fixture(public_input_count: usize) -> Result<Groth16SerializedFixture, String> {
    let public_input = groth16_high_hamming_public_input();

    let mut rng = StdRng::seed_from_u64(public_input_count as u64);
    let circuit = Groth16PublicInputCircuit { public_input_count, public_input };
    let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit, &mut rng)
        .map_err(|err| format!("failed to setup groth16 public-input fixture: {err}"))?;

    let circuit = Groth16PublicInputCircuit { public_input_count, public_input };
    let proof = Groth16::<Bn254>::prove(&pk, circuit, &mut rng)
        .map_err(|err| format!("failed to prove groth16 public-input fixture: {err}"))?;

    let mut vk_bytes = Vec::new();
    vk.serialize_compressed(&mut vk_bytes).map_err(|err| format!("failed to serialize groth16 public-input fixture vk: {err}"))?;

    let mut proof_bytes = Vec::new();
    proof
        .serialize_compressed(&mut proof_bytes)
        .map_err(|err| format!("failed to serialize groth16 public-input fixture proof: {err}"))?;

    let mut public_input_bytes = Vec::new();
    public_input
        .serialize_uncompressed(&mut public_input_bytes)
        .map_err(|err| format!("failed to serialize groth16 public input: {err}"))?;

    Ok(Groth16SerializedFixture { vk_bytes, proof_bytes, public_input_bytes })
}

fn build_groth16_repeated_valid_script(public_input_count: usize, call_count: usize) -> Result<Vec<u8>, String> {
    let fixture = build_groth16_valid_fixture(public_input_count)?;
    let groth16_script =
        build_groth16_repeated_input_script(public_input_count, &fixture.vk_bytes, &fixture.proof_bytes, &fixture.public_input_bytes)?;
    Ok(build_repeated_groth16_script(&groth16_script, call_count))
}

fn groth16_extended_vk_bytes(vk_bytes: &[u8], vk_gamma_abc_count: usize) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut vk = VerifyingKey::<Bn254>::deserialize_compressed(vk_bytes)
        .map_err(|err| format!("failed to deserialize groth16 verifying key: {err}"))?;
    let gamma_point =
        vk.gamma_abc_g1.last().cloned().ok_or_else(|| "groth16 verifying key should contain gamma_abc_g1".to_string())?;

    let mut gamma_point_bytes = Vec::new();
    gamma_point
        .serialize_compressed(&mut gamma_point_bytes)
        .map_err(|err| format!("failed to serialize groth16 gamma_abc_g1 point: {err}"))?;

    vk.gamma_abc_g1.resize(vk_gamma_abc_count, gamma_point);
    let mut extended_vk_bytes = Vec::new();
    vk.serialize_compressed(&mut extended_vk_bytes).map_err(|err| format!("failed to serialize groth16 verifying key: {err}"))?;

    Ok((extended_vk_bytes, gamma_point_bytes))
}

fn add_groth16_repeated_vk_tail(builder: &mut ScriptBuilder, gamma_point_bytes: &[u8], repeat_count: usize) -> Result<(), String> {
    let mut has_accumulated_tail = false;
    for bit in (0..usize::BITS as usize).rev() {
        if repeat_count & (1usize << bit) == 0 {
            continue;
        }

        builder.add_data(gamma_point_bytes).map_err(|err| format!("failed to add groth16 gamma_abc_g1 point: {err}"))?;
        for _ in 0..bit {
            builder.add_op(OpDup).map_err(|err| format!("failed to duplicate groth16 gamma_abc_g1 tail: {err}"))?;
            builder.add_op(codes::OpCat).map_err(|err| format!("failed to double groth16 gamma_abc_g1 tail: {err}"))?;
        }
        if has_accumulated_tail {
            builder.add_op(codes::OpCat).map_err(|err| format!("failed to append groth16 gamma_abc_g1 tail: {err}"))?;
        }
        has_accumulated_tail = true;
    }

    Ok(())
}

fn groth16_large_vk_public_input_count() -> usize {
    groth16_max_pub_input_count().saturating_sub(2).max(1)
}

fn add_groth16_large_vk_compute_padding(
    builder: &mut ScriptBuilder,
    padding_chunks: usize,
    padding_bytes: usize,
) -> Result<(), String> {
    for _ in 0..padding_chunks {
        builder.add_data(&[0x42]).map_err(|err| format!("failed to add groth16 large-vk padding seed: {err}"))?;
        for _ in 0..GROTH16_LARGE_VK_PADDING_CAT_COUNT {
            builder.add_op(OpDup).map_err(|err| format!("failed to duplicate groth16 large-vk padding: {err}"))?;
            builder.add_op(codes::OpCat).map_err(|err| format!("failed to double groth16 large-vk padding: {err}"))?;
        }
        builder.add_op(OpDrop).map_err(|err| format!("failed to drop groth16 large-vk padding: {err}"))?;
    }

    if padding_bytes > 0 {
        builder.add_data(&vec![0x24; padding_bytes]).map_err(|err| format!("failed to add groth16 large-vk byte padding: {err}"))?;
        builder.add_op(OpDrop).map_err(|err| format!("failed to drop groth16 large-vk byte padding: {err}"))?;
    }

    Ok(())
}

fn build_groth16_prepare_inputs_large_vk_script(
    vk_gamma_abc_count: usize,
    padding_chunks: usize,
    padding_bytes: usize,
) -> Result<Vec<u8>, String> {
    let public_input_count = groth16_large_vk_public_input_count();
    assert!(
        vk_gamma_abc_count > public_input_count,
        "groth16 verifying key should contain one gamma_abc_g1 base point plus all public inputs"
    );

    let (vk_bytes, proof_bytes, inputs) = load_groth_fields();
    let input = inputs.first().ok_or_else(|| "groth16 fixture should contain at least one public input".to_string())?;
    let (extended_vk_bytes, gamma_point_bytes) = groth16_extended_vk_bytes(vk_bytes.as_slice(), vk_gamma_abc_count)?;
    let matching_gamma_abc_count = public_input_count + 1;
    let extra_gamma_abc_count = vk_gamma_abc_count - matching_gamma_abc_count;
    let repeated_tail_len = extra_gamma_abc_count
        .checked_mul(gamma_point_bytes.len())
        .ok_or_else(|| "groth16 gamma_abc_g1 tail length overflowed".to_string())?;
    let prefix_len = extended_vk_bytes
        .len()
        .checked_sub(repeated_tail_len)
        .ok_or_else(|| "groth16 gamma_abc_g1 tail is longer than the verifying key".to_string())?;
    let repeated_tail = &extended_vk_bytes[prefix_len..];

    if extra_gamma_abc_count > 0
        && !repeated_tail.chunks_exact(gamma_point_bytes.len()).all(|chunk| chunk == gamma_point_bytes.as_slice())
    {
        return Err("groth16 verifying key tail is not made of repeated gamma_abc_g1 points".to_string());
    }

    let mut builder = new_script_builder();
    add_groth16_large_vk_compute_padding(&mut builder, padding_chunks, padding_bytes)?;

    builder.add_data(input).map_err(|err| format!("failed to add groth16 public input: {err}"))?;
    for _ in 1..public_input_count {
        builder.add_op(OpDup).map_err(|err| format!("failed to duplicate groth16 public input: {err}"))?;
    }
    builder
        .add_i64(public_input_count as i64)
        .map_err(|err| format!("failed to add groth16 public input count: {err}"))?
        .add_data(&proof_bytes)
        .map_err(|err| format!("failed to add groth16 proof: {err}"))?;

    if extra_gamma_abc_count == 0 {
        builder.add_data(&extended_vk_bytes).map_err(|err| format!("failed to add groth16 verifying key: {err}"))?;
    } else {
        builder
            .add_data(&extended_vk_bytes[..prefix_len])
            .map_err(|err| format!("failed to add groth16 verifying key prefix: {err}"))?;
        add_groth16_repeated_vk_tail(&mut builder, &gamma_point_bytes, extra_gamma_abc_count)?;
        builder.add_op(codes::OpCat).map_err(|err| format!("failed to assemble groth16 verifying key: {err}"))?;
    }

    builder
        .add_data(&[ZkTag::Groth16 as u8])
        .map_err(|err| format!("failed to add groth16 tag: {err}"))?
        .add_op(codes::OpZkPrecompile)
        .map_err(|err| format!("failed to add groth16 opcode: {err}"))?;

    Ok(builder.drain())
}

fn try_build_groth16_prepare_inputs_tx_with_input_count(
    nonce: u32,
    public_input_count: usize,
) -> Result<(Transaction, Vec<UtxoEntry>), String> {
    let script = build_groth16_prepare_inputs_script(public_input_count, public_input_count + 1)?;
    try_build_budgeted_single_input_tx_allowing_zk_failure(nonce, ScriptPublicKey::new(0, script.into()), vec![])
}

fn try_build_groth16_prepare_inputs_large_vk_tx_with_gamma_count(
    nonce: u32,
    vk_gamma_abc_count: usize,
) -> Result<(Transaction, Vec<UtxoEntry>), String> {
    try_build_groth16_large_vk_tx_with_params(nonce, vk_gamma_abc_count, 0, 0)
}

fn try_build_groth16_large_vk_tx_with_params(
    nonce: u32,
    vk_gamma_abc_count: usize,
    padding_chunks: usize,
    padding_bytes: usize,
) -> Result<(Transaction, Vec<UtxoEntry>), String> {
    let redeem_script = build_groth16_prepare_inputs_large_vk_script(vk_gamma_abc_count, padding_chunks, padding_bytes)?;
    let signature_script = pay_to_script_hash_signature_script_with_flags(redeem_script.clone(), vec![], pricing_flags(true))
        .map_err(|err| format!("failed to build groth16 large-vk p2sh signature script: {err}"))?;
    try_build_budgeted_single_input_tx_allowing_zk_failure(nonce, pay_to_script_hash_script(&redeem_script), signature_script)
}

fn groth16_max_pub_input_count() -> usize {
    static INPUT_COUNT: OnceLock<usize> = OnceLock::new();
    *INPUT_COUNT.get_or_init(|| {
        fn valid_input_count(public_input_count: usize) -> bool {
            let Ok(candidate) = try_build_groth16_prepare_inputs_tx_with_input_count(0, public_input_count) else {
                return false;
            };
            fits_block_mass(&candidate.0)
        }

        let mut low_input_count = 1usize;
        let mut high_input_count = 2usize;
        assert!(valid_input_count(low_input_count), "single groth16 public input should be valid");

        loop {
            if !valid_input_count(high_input_count) {
                break;
            }
            low_input_count = high_input_count;
            let next_high_input_count = high_input_count.saturating_mul(2);
            if next_high_input_count == high_input_count {
                break;
            }
            high_input_count = next_high_input_count;
        }

        while low_input_count + 1 < high_input_count {
            let mid_input_count = low_input_count + (high_input_count - low_input_count) / 2;
            if valid_input_count(mid_input_count) {
                low_input_count = mid_input_count;
            } else {
                high_input_count = mid_input_count;
            }
        }

        low_input_count
    })
}

fn build_groth16_1x_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let public_input_count = groth16_max_pub_input_count();
    let candidate = try_build_groth16_prepare_inputs_tx_with_input_count(nonce, public_input_count)
        .expect("cached groth16 public input count should be valid");
    assert!(fits_block_mass(&candidate.0), "cached groth16 public input count should stay within block mass limits");
    candidate
}

fn estimate_groth16_repeated_tx_with_input_count(
    nonce: u32,
    public_input_count: usize,
    call_count: usize,
) -> Option<(Transaction, Vec<UtxoEntry>)> {
    let single_candidate = try_build_groth16_prepare_inputs_tx_with_input_count(nonce, public_input_count).ok()?;
    let TxInputMass::ComputeBudget(single_budget) = single_candidate.0.inputs.first()?.mass else {
        return None;
    };
    let repeated_budget = single_budget.value().checked_mul(u16::try_from(call_count).ok()?)?.checked_add(1).map(ComputeBudget)?;
    let script = build_groth16_prepare_inputs_repeated_script(public_input_count, call_count).ok()?;
    Some(build_single_input_tx_with_compute_budget(nonce, ScriptPublicKey::new(0, script.into()), vec![], repeated_budget))
}

fn groth16_repeated_pub_input_count(call_count: usize) -> usize {
    fn search(call_count: usize) -> usize {
        fn valid_input_count_with_call_count(call_count: usize, public_input_count: usize) -> bool {
            let Some(candidate) = estimate_groth16_repeated_tx_with_input_count(0, public_input_count, call_count) else {
                return false;
            };
            fits_block_mass(&candidate.0)
        }

        let mut low_input_count = 1usize;
        let mut high_input_count = 2usize;
        assert!(
            valid_input_count_with_call_count(call_count, low_input_count),
            "single groth16 repeated public input should be valid"
        );

        loop {
            if !valid_input_count_with_call_count(call_count, high_input_count) {
                break;
            }
            low_input_count = high_input_count;
            let next_high_input_count = high_input_count.saturating_mul(2);
            if next_high_input_count == high_input_count {
                break;
            }
            high_input_count = next_high_input_count;
        }

        high_input_count = high_input_count.min(groth16_max_pub_input_count().saturating_add(1));
        while low_input_count + 1 < high_input_count {
            let mid_input_count = low_input_count + (high_input_count - low_input_count) / 2;
            if valid_input_count_with_call_count(call_count, mid_input_count) {
                low_input_count = mid_input_count;
            } else {
                high_input_count = mid_input_count;
            }
        }

        low_input_count
    }

    match call_count {
        GROTH16_2X_COUNT => {
            static INPUT_COUNT: OnceLock<usize> = OnceLock::new();
            *INPUT_COUNT.get_or_init(|| search(call_count))
        }
        GROTH16_3X_COUNT => {
            static INPUT_COUNT: OnceLock<usize> = OnceLock::new();
            *INPUT_COUNT.get_or_init(|| search(call_count))
        }
        _ => search(call_count),
    }
}

fn groth16_2x_pub_input_count() -> usize {
    groth16_repeated_pub_input_count(GROTH16_2X_COUNT)
}

fn groth16_3x_pub_input_count() -> usize {
    groth16_repeated_pub_input_count(GROTH16_3X_COUNT)
}

fn build_groth16_2x_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    static SCRIPT: OnceLock<Vec<u8>> = OnceLock::new();
    let script = SCRIPT
        .get_or_init(|| {
            let public_input_count = groth16_2x_pub_input_count();
            build_groth16_repeated_valid_script(public_input_count, GROTH16_2X_COUNT)
                .expect("cached groth16 2x public input count should be valid")
        })
        .clone();
    let candidate = try_build_budgeted_single_input_tx(nonce, ScriptPublicKey::new(0, script.into()), vec![])
        .expect("cached groth16 2x script should be valid");
    assert!(fits_block_mass(&candidate.0), "cached groth16 2x public input count should stay within block mass limits");
    candidate
}

fn groth16_large_vk_gamma_abc_count() -> usize {
    static GAMMA_ABC_COUNT: OnceLock<usize> = OnceLock::new();
    *GAMMA_ABC_COUNT.get_or_init(|| {
        fn valid_gamma_abc_count(vk_gamma_abc_count: usize) -> bool {
            let Ok(candidate) = try_build_groth16_prepare_inputs_large_vk_tx_with_gamma_count(0, vk_gamma_abc_count) else {
                return false;
            };
            fits_block_mass(&candidate.0)
        }

        let mut low_gamma_abc_count = groth16_max_pub_input_count() + 1;
        let mut high_gamma_abc_count = low_gamma_abc_count.saturating_mul(2);
        assert!(valid_gamma_abc_count(low_gamma_abc_count), "matching groth16 public input and gamma_abc_g1 count should be valid");

        loop {
            if !valid_gamma_abc_count(high_gamma_abc_count) {
                break;
            }
            low_gamma_abc_count = high_gamma_abc_count;
            let next_high_gamma_abc_count = high_gamma_abc_count.saturating_mul(2);
            if next_high_gamma_abc_count == high_gamma_abc_count {
                break;
            }
            high_gamma_abc_count = next_high_gamma_abc_count;
        }

        while low_gamma_abc_count + 1 < high_gamma_abc_count {
            let mid_gamma_abc_count = low_gamma_abc_count + (high_gamma_abc_count - low_gamma_abc_count) / 2;
            if valid_gamma_abc_count(mid_gamma_abc_count) {
                low_gamma_abc_count = mid_gamma_abc_count;
            } else {
                high_gamma_abc_count = mid_gamma_abc_count;
            }
        }

        low_gamma_abc_count
    })
}

fn groth16_large_vk_padding() -> (usize, usize) {
    static PADDING: OnceLock<(usize, usize)> = OnceLock::new();
    *PADDING.get_or_init(|| {
        let vk_gamma_abc_count = groth16_large_vk_gamma_abc_count();

        fn valid_padding(vk_gamma_abc_count: usize, padding_chunks: usize, padding_bytes: usize) -> bool {
            let Ok(candidate) = try_build_groth16_large_vk_tx_with_params(0, vk_gamma_abc_count, padding_chunks, padding_bytes) else {
                return false;
            };
            fits_block_mass(&candidate.0)
        }

        let mut low_chunks = 0usize;
        let mut high_chunks = 1usize;

        loop {
            if !valid_padding(vk_gamma_abc_count, high_chunks, 0) {
                break;
            }
            low_chunks = high_chunks;
            let next_high_chunks = high_chunks.saturating_mul(2);
            if next_high_chunks == high_chunks {
                break;
            }
            high_chunks = next_high_chunks;
        }

        while low_chunks + 1 < high_chunks {
            let mid_chunks = low_chunks + (high_chunks - low_chunks) / 2;
            if valid_padding(vk_gamma_abc_count, mid_chunks, 0) {
                low_chunks = mid_chunks;
            } else {
                high_chunks = mid_chunks;
            }
        }

        let mut low_bytes = 0usize;
        let mut high_bytes = max_script_element_size(true).saturating_add(1);
        while low_bytes + 1 < high_bytes {
            let mid_bytes = low_bytes + (high_bytes - low_bytes) / 2;
            if valid_padding(vk_gamma_abc_count, low_chunks, mid_bytes) {
                low_bytes = mid_bytes;
            } else {
                high_bytes = mid_bytes;
            }
        }

        (low_chunks, low_bytes)
    })
}

fn build_groth16_large_vk_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    let vk_gamma_abc_count = groth16_large_vk_gamma_abc_count();
    let (padding_chunks, padding_bytes) = groth16_large_vk_padding();
    let candidate = try_build_groth16_large_vk_tx_with_params(nonce, vk_gamma_abc_count, padding_chunks, padding_bytes)
        .expect("cached groth16 large-vk gamma_abc_g1 count should be valid");
    assert!(fits_block_mass(&candidate.0), "cached groth16 large-vk gamma_abc_g1 count should stay within block mass limits");
    candidate
}

fn build_large_push_dup_cat_tx_with_data_len(nonce: u32, data_len: usize) -> (Transaction, Vec<UtxoEntry>) {
    let signature_script = new_script_builder().add_data(&vec![0x6du8; data_len]).unwrap().drain();
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

    assert_eq!(LARGE_PUSH_DUP_CAT_DUP_COUNT + 1, MAX_STACK_SIZE, "large_push_dup_cat should reach the stack depth limit");
    assert!(
        low_len * LARGE_PUSH_DUP_CAT_EXPANSION_FACTOR <= max_script_element_size(true),
        "large_push_dup_cat expanded element should stay within the element size limit"
    );

    best
}

fn build_op_dup_one_tx_script_public_key(nonce: u32) -> ScriptPublicKey {
    // Adds OpDrop/OpDup pairs to increase opcode work while keeping stack depth stable.
    fn script_for_opcode_pairs(opcode_pairs: usize) -> ScriptPublicKey {
        let mut builder = new_script_builder();
        builder.add_i64(1).unwrap();
        for _ in 0..OP_DUP_BASE_DUP_COUNT {
            builder.add_op(OpDup).unwrap();
        }
        for _ in 0..opcode_pairs {
            builder.add_op(OpDrop).unwrap();
            builder.add_op(OpDup).unwrap();
        }
        for _ in 0..OP_DUP_BASE_DUP_COUNT {
            builder.add_op(OpDrop).unwrap();
        }
        ScriptPublicKey::new(0, builder.drain().into())
    }

    fn valid_candidate(nonce: u32, opcode_pairs: usize) -> Option<ScriptPublicKey> {
        let candidate_script = script_for_opcode_pairs(opcode_pairs);
        let Ok((candidate_tx, _)) = try_build_budgeted_single_input_tx(nonce, candidate_script.clone(), vec![]) else {
            return None;
        };
        if compute_mass(&candidate_tx) > BLOCK_COMPUTE_MASS_LIMIT {
            return None;
        }
        Some(candidate_script)
    }

    let mut low_opcode_pairs = 0usize;
    let mut high_opcode_pairs = OP_DUP_ONE_TX_PAIR_SEARCH_STEP;
    let mut best_script = valid_candidate(nonce, low_opcode_pairs).expect("base op_dup_one_tx script should be valid");

    loop {
        let Some(candidate_script) = valid_candidate(nonce, high_opcode_pairs) else {
            break;
        };
        best_script = candidate_script;
        low_opcode_pairs = high_opcode_pairs;
        high_opcode_pairs = high_opcode_pairs.saturating_mul(2);
    }

    while low_opcode_pairs + 1 < high_opcode_pairs {
        let mid_opcode_pairs = low_opcode_pairs + (high_opcode_pairs - low_opcode_pairs) / 2;
        if let Some(candidate_script) = valid_candidate(nonce, mid_opcode_pairs) {
            best_script = candidate_script;
            low_opcode_pairs = mid_opcode_pairs;
        } else {
            high_opcode_pairs = mid_opcode_pairs;
        }
    }

    best_script
}

fn build_op_dup_one_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    build_budgeted_single_input_tx(nonce, build_op_dup_one_tx_script_public_key(nonce), vec![])
}

fn build_max_opcodes_tx(nonce: u32) -> (Transaction, Vec<UtxoEntry>) {
    fn try_tx_with_opcode_count(nonce: u32, opcode_count: usize) -> Option<(Transaction, Vec<UtxoEntry>)> {
        let redeem_script = ScriptPublicKey::new(0, vec![codes::OpNop; opcode_count].into());
        let signature_script = new_p2sh_signature_script(redeem_script.script().to_vec(), vec![codes::OpTrue]);
        try_build_budgeted_single_input_tx(nonce, pay_to_script_hash_script(redeem_script.script()), signature_script).ok()
    }

    let mut low_count = 0usize;
    let mut high_count = OP_DUP_ONE_TX_PAIR_SEARCH_STEP;
    let mut best = try_tx_with_opcode_count(nonce, low_count).expect("empty max_opcodes script should be valid");

    loop {
        let Some(candidate) = try_tx_with_opcode_count(nonce, high_count) else {
            break;
        };
        if !fits_block_mass(&candidate.0) {
            break;
        }
        best = candidate;
        low_count = high_count;
        high_count = high_count.saturating_mul(2);
    }

    while low_count + 1 < high_count {
        let mid_count = low_count + (high_count - low_count) / 2;
        match try_tx_with_opcode_count(nonce, mid_count) {
            Some(candidate) if fits_block_mass(&candidate.0) => {
                low_count = mid_count;
                best = candidate;
            }
            _ => high_count = mid_count,
        }
    }

    best
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
    pack_repeated_txs_with_zk_failure(name, builder, false)
}

fn pack_repeated_txs_with_zk_failure(name: &'static str, builder: TxBuilder, allow_zk_failure: bool) -> BenchBlock {
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
        allow_zk_failure,
        txs,
    }
}

fn single_tx_block(name: &'static str, tx: Transaction, entries: Vec<UtxoEntry>) -> BenchBlock {
    single_tx_block_with_zk_failure(name, tx, entries, false)
}

fn single_tx_block_with_zk_failure(
    name: &'static str,
    tx: Transaction,
    entries: Vec<UtxoEntry>,
    allow_zk_failure: bool,
) -> BenchBlock {
    BenchBlock {
        name,
        tx_count: 1,
        input_count: tx.inputs.len(),
        compute_mass: compute_mass(&tx),
        transient_mass: transient_mass(&tx),
        allow_zk_failure,
        txs: vec![prepare_bench_tx(tx, entries)],
    }
}

fn fixed_txs_block(name: &'static str, tx_entries: Vec<(Transaction, Vec<UtxoEntry>)>) -> BenchBlock {
    let tx_count = tx_entries.len();
    let input_count = tx_entries.iter().map(|(tx, _)| tx.inputs.len()).sum();
    let compute_mass = tx_entries.iter().map(|(tx, _)| compute_mass(tx)).sum();
    let transient_mass = tx_entries.iter().map(|(tx, _)| transient_mass(tx)).sum();
    let txs = tx_entries.into_iter().map(|(tx, entries)| prepare_bench_tx(tx, entries)).collect();

    BenchBlock { name, txs, tx_count, input_count, compute_mass, transient_mass, allow_zk_failure: false }
}

fn build_schnorr_block() -> BenchBlock {
    pack_repeated_txs("schnorr_2in1", build_schnorr_2in1_tx)
}

fn build_ecdsa_block() -> BenchBlock {
    pack_repeated_txs("ecdsa_2in1", build_ecdsa_2in1_tx)
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

fn build_max_opcodes_block() -> BenchBlock {
    let (tx, entries) = build_max_opcodes_tx(0);
    let tx_compute_mass = compute_mass(&tx);
    let tx_transient_mass = transient_mass(&tx);
    assert!(
        tx_compute_mass <= BLOCK_COMPUTE_MASS_LIMIT,
        "max_opcodes compute mass {tx_compute_mass} exceeds block limit {BLOCK_COMPUTE_MASS_LIMIT}"
    );
    assert!(
        tx_transient_mass <= BLOCK_TRANSIENT_MASS_LIMIT,
        "max_opcodes transient mass {tx_transient_mass} exceeds block limit {BLOCK_TRANSIENT_MASS_LIMIT}"
    );
    single_tx_block("max_opcodes", tx, entries)
}

fn build_single_stark_block() -> BenchBlock {
    let (tx, entries) = build_single_stark_tx(0);
    single_tx_block("single_stark", tx, entries)
}

fn build_groth16_3tx_block() -> BenchBlock {
    fixed_txs_block("groth16_3tx", vec![build_groth16_tx(0), build_groth16_tx(1), build_groth16_tx(2)])
}

fn build_groth16_3x_block() -> BenchBlock {
    let (tx, entries) = build_groth16_3x_tx(0);
    single_tx_block("groth16_3x", tx, entries)
}

fn build_groth16_1x_block() -> BenchBlock {
    pack_repeated_txs_with_zk_failure("groth16_1x", build_groth16_1x_tx, true)
}

fn build_groth16_2x_block() -> BenchBlock {
    pack_repeated_txs("groth16_2x", build_groth16_2x_tx)
}

fn build_groth16_large_vk_block() -> BenchBlock {
    pack_repeated_txs_with_zk_failure("groth16_large_vk", build_groth16_large_vk_tx, true)
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
            build_ecdsa_block(),
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
            build_max_opcodes_block(),
            build_single_stark_block(),
            build_groth16_3tx_block(),
            build_groth16_3x_block(),
            build_groth16_2x_block(),
            build_groth16_1x_block(),
            build_groth16_large_vk_block(),
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

fn accept_validation_result(result: Result<(), TxScriptError>, allow_zk_failure: bool) -> Result<(), TxScriptError> {
    match result {
        Ok(()) => Ok(()),
        Err(TxScriptError::ZkIntegrity(_)) if allow_zk_failure => Ok(()),
        Err(err) => Err(err),
    }
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
            accept_validation_result(vm.execute(), block.allow_zk_failure).unwrap();
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
                accept_validation_result(vm.execute(), block.allow_zk_failure)
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
