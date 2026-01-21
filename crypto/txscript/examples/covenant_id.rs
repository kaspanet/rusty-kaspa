use kaspa_consensus_core::hashing;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::{
    CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
    UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_txscript::caches::Cache;
use kaspa_txscript::opcodes::codes::{
    Op1Add, OpBlake2b, OpCat, OpCovOutputCount, OpCovOutputIdx, OpData62, OpData8, OpEqual, OpEqualVerify, OpNum2Bin, OpSwap, OpTrue,
    OpTxInputIndex, OpTxInputScriptSigLen, OpTxInputScriptSigSubstr, OpTxOutputSpkLen, OpTxOutputSpkSubstr,
};
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};
use kaspa_txscript::{pay_to_script_hash_script, EngineCtx};
use kaspa_txscript::{EngineFlags, TxScriptEngine};
use kaspa_txscript_errors::TxScriptError;
use rand::{seq::SliceRandom, Rng, RngCore, SeedableRng};

fn main() -> ScriptBuilderResult<()> {
    counter_state_in_spk()
}

/// Covenant that keeps the counter in the script public key using state inside P2SH.
/// Each spend must increment the counter and rebind the funds to the same covenant script
/// with the updated state hash embedded in the script public key.
fn counter_state_in_spk() -> ScriptBuilderResult<()> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(0);
    println!("[COVENANT P2SH-WS] Counter stored in script public key");
    let covenant_script = build_covenant_script()?;

    // Shared engine state
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    // Create the initial UTXO with counter = 0
    let mut state = CovenantState::new(0, &covenant_script);

    // Two valid increments
    for next in [1u8, 2u8] {
        println!("[COVENANT P2SH-WS] Spending to counter {next}");
        let tx = build_spend_tx(&state, next, &covenant_script, &mut rng);
        run_vm(&tx, &state.utxo_entry, &sig_cache, &reused_values, flags).unwrap();
        state = CovenantState::from_tx(tx, &covenant_script, next);
    }

    let counter_2_state = state.clone();
    let next = 3u8;
    println!("[COVENANT P2SH-WS] Spending to counter {next}");
    let tx = build_spend_tx(&state, next, &covenant_script, &mut rng);
    run_vm(&tx, &state.utxo_entry, &sig_cache, &reused_values, flags).unwrap();
    state = CovenantState::from_tx(tx, &covenant_script, next);

    println!("[COVENANT P2SH-WS] Attempting invalid spend (no increment)");
    let bad_tx = build_spend_tx(&state, state.counter, &covenant_script, &mut rng);
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT P2SH-WS] Expected failure: {err:?}");

    println!("[COVENANT P2SH-WS] Attempting invalid spend (reuse previous state)");
    // We try to spend the last UTXO but provide the previous state with counter=2
    let bad_tx = build_spend_tx(
        &CovenantState { utxo_outpoint: state.utxo_outpoint, ..counter_2_state },
        state.counter,
        &covenant_script,
        &mut rng,
    );
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT P2SH-WS] Expected failure: {err:?}");

    println!("[COVENANT P2SH-WS] Attempting invalid spend (increase by 2)");
    let bad_tx = build_spend_tx(&state, state.counter + 2, &covenant_script, &mut rng);
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT P2SH-WS] Expected failure: {err:?}");

    println!("[COVENANT P2SH-WS] Example complete");
    Ok(())
}

/// Holds the current covenant UTXO state.
#[derive(Clone)]
struct CovenantState {
    utxo_outpoint: TransactionOutpoint,
    utxo_entry: UtxoEntry,
    counter: u8,
    covenant_id: Hash,
}

impl CovenantState {
    fn new(counter: u8, covenant_script: &[u8]) -> Self {
        let tx = genesis_tx(counter, covenant_script);
        Self::from_tx(tx, covenant_script, counter)
    }

    fn from_tx(tx: Transaction, covenant_script: &[u8], counter: u8) -> Self {
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let spk = build_spk(counter, covenant_script);
        let utxo_entry = UtxoEntry::new(1_000_000, spk, 0, false, None);
        Self { utxo_outpoint: outpoint, utxo_entry, counter, covenant_id: hashing::covenant_id::covenant_id(outpoint) }
    }
}

/// Build the covenant script that enforces:
/// 1) The counter is incremented (state -> state+1).
/// 2) The spend has exactly one authorized output.
/// 3) The output script public key matches the same redeem script suffix and embeds the hash of (state+1) in the state slot.
fn build_covenant_script() -> ScriptBuilderResult<Vec<u8>> {
    let p2sh_prefix = [
        0,
        0, // Script version 0 (two bytes)
        kaspa_txscript::opcodes::codes::OpBlake2b,
        kaspa_txscript::opcodes::codes::OpData32,
    ];
    let p2sh_suffix = [kaspa_txscript::opcodes::codes::OpEqual];

    Ok(ScriptBuilder::new()
        // Check that this is the first input.
        .add_op(OpTxInputIndex)?
        .add_i64(0)?
        .add_op(OpEqualVerify)?

        // Check that there is exactly one authorized output
        .add_i64(0)?
        .add_op(OpCovOutputCount)?
        .add_i64(1)?
        .add_op(OpEqualVerify)?

        // Check that the sigScript pushes a redeem script with the expected length
        .add_i64(0)? // Input index
        .add_i64(0)? // Start
        .add_i64(1)? // End
        .add_op(OpTxInputScriptSigSubstr)?
        .add_data(&[OpData62])?
        .add_op(OpEqualVerify)?

        // Check that the redeem script starts by pushing 8 bytes (the counter)
        .add_i64(0)? // Input index
        .add_i64(1)? // Start
        .add_i64(2)? // End
        .add_op(OpTxInputScriptSigSubstr)?
        .add_data(&[OpData8])?
        .add_op(OpEqualVerify)?

        // ------ State transition start ------
        // Increment the counter
        .add_op(Op1Add)?

        // Expand to 8 bytes
        .add_i64(8)?
        .add_op(OpNum2Bin)?
        // ------ State transition end ------

        // Add OpData8 prefix
        .add_data(&[OpData8])?
        .add_op(OpSwap)?
        .add_op(OpCat)?


        // Fetch the redeem script suffix (after the counter)
        .add_i64(0)? // Input index
        .add_i64(10)? // Start
        .add_i64(0)?
        .add_op(OpTxInputScriptSigLen)? // End
        .add_op(OpTxInputScriptSigSubstr)?

        // Reconstruct the expected redeem script with incremented counter
        .add_op(OpCat)?

        // Hash the redeem script
        .add_op(OpBlake2b)?

        // Reconstruct the expected P2SH scriptPubKey
        .add_data(&p2sh_prefix)?
        .add_op(OpSwap)?
        .add_op(OpCat)?
        .add_data(&p2sh_suffix)?
        .add_op(OpCat)?

        // Compare to the output scriptPubKey
        .add_i64(0)?
        .add_i64(0)?
        .add_op(OpCovOutputIdx)? // Output index
        .add_i64(0)? // Start
        .add_i64(0)?
        .add_i64(0)?
        .add_op(OpCovOutputIdx)?
        .add_op(OpTxOutputSpkLen)? // End
        .add_op(OpTxOutputSpkSubstr)?
        .add_op(OpEqual)?

        .drain())
}

/// Build the spend transaction for the next counter value.
fn build_spend_tx(state: &CovenantState, next_counter: u8, covenant_script: &[u8], rng: &mut impl RngCore) -> Transaction {
    let sig_script = ScriptBuilder::new().add_data(&build_redeem_script(state.counter, covenant_script)).unwrap().drain();

    let input = TransactionInput::new(state.utxo_outpoint, sig_script, 0, 0);
    let mut output = TransactionOutput::new(state.utxo_entry.amount - 10, build_spk(next_counter, covenant_script));
    output.covenant = Some(CovenantBinding { covenant_id: state.covenant_id, authorizing_input: 0 });

    let mut outputs = vec![output];

    let num_additional_outputs = rng.gen_range(0..4);
    for _ in 0..num_additional_outputs {
        let dummy_spk = pay_to_script_hash_script(&[OpTrue]); // P2SH of OP_TRUE
        let dummy_output = TransactionOutput::new(1, dummy_spk);
        outputs.push(dummy_output);
    }

    // We check that the covenant script correctly identifies its authorized output, no matter the order or the amount.
    outputs.shuffle(&mut *rng);

    let mut tx = Transaction::new(0, vec![input], outputs, 0, SubnetworkId::default(), 0, vec![]);
    tx.finalize();
    tx
}

/// Run the VM for a single-input covenant spend.
fn run_vm(
    tx: &Transaction,
    utxo_entry: &UtxoEntry,
    sig_cache: &Cache<kaspa_txscript::SigCacheKey, bool>,
    reused_values: &SigHashReusedValuesUnsync,
    flags: EngineFlags,
) -> Result<(), TxScriptError> {
    let populated = PopulatedTransaction::new(tx, vec![utxo_entry.clone()]);
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[0],
        0,
        utxo_entry,
        EngineCtx::new(sig_cache).with_reused(reused_values),
        flags,
    );
    vm.execute()
}

/// Create a genesis-style transaction that seeds the first covenant UTXO.
fn genesis_tx(counter: u8, covenant_script: &[u8]) -> Transaction {
    let dummy_input = TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(0), 0), vec![], 0, 0);
    let output = TransactionOutput::new(1_000_000, build_spk(counter, covenant_script));
    let mut tx = Transaction::new(0, vec![dummy_input], vec![output], 0, SubnetworkId::default(), 0, vec![]);
    tx.finalize();
    tx
}

fn build_spk(counter: u8, covenant_script: &[u8]) -> ScriptPublicKey {
    pay_to_script_hash_script(&build_redeem_script(counter, covenant_script))
}

fn build_redeem_script(counter: u8, covenant_script: &[u8]) -> Vec<u8> {
    let mut redeem_script = ScriptBuilder::new().add_data(&encode_counter(counter)).unwrap().drain();
    redeem_script.extend(covenant_script.iter().cloned());
    redeem_script
}

fn encode_counter(counter: u8) -> Vec<u8> {
    let mut v = vec![0u8; 8];
    v[0] = counter;
    v
}
