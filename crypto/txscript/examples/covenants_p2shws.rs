use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_txscript::caches::Cache;
use kaspa_txscript::opcodes::codes::{
    Op1Add, OpAdd, OpBlake2b, OpDrop, OpDup, OpEqual, OpEqualVerify, OpSub, OpSwap, OpTrue, OpTxInputSpkLen, OpTxInputSpkSubstr,
    OpTxOutputCount, OpTxOutputSpkLen, OpTxOutputSpkSubstr,
};
use kaspa_txscript::pay_to_script_hash_with_state;
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};
use kaspa_txscript::{EngineFlags, TxScriptEngine};
use kaspa_txscript_errors::TxScriptError;

fn main() -> ScriptBuilderResult<()> {
    counter_state_in_spk()
}

/// Covenant that keeps the counter in the script public key using P2SH-with-state.
/// Each spend must increment the counter and rebind the funds to the same covenant script
/// with the updated state hash embedded in the script public key.
fn counter_state_in_spk() -> ScriptBuilderResult<()> {
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
        let tx = build_spend_tx(&state, next, &covenant_script);
        run_vm(&tx, &state.utxo_entry, &sig_cache, &reused_values, flags).unwrap();
        state = CovenantState::from_tx(tx, &covenant_script, next);
    }

    let counter_2_state = state.clone();
    let next = 3u8;
    println!("[COVENANT P2SH-WS] Spending to counter {next}");
    let tx = build_spend_tx(&state, next, &covenant_script);
    run_vm(&tx, &state.utxo_entry, &sig_cache, &reused_values, flags).unwrap();
    state = CovenantState::from_tx(tx, &covenant_script, next);

    println!("[COVENANT P2SH-WS] Attempting invalid spend (no increment)");
    let bad_tx = build_spend_tx(&state, state.counter, &covenant_script);
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT P2SH-WS] Expected failure: {err:?}");

    println!("[COVENANT P2SH-WS] Attempting invalid spend (reuse previous state)");
    // We try to spend the last UTXO but provide the previous state with counter=2
    let bad_tx =
        build_spend_tx(&CovenantState { utxo_outpoint: state.utxo_outpoint, ..counter_2_state }, state.counter, &covenant_script);
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT P2SH-WS] Expected failure: {err:?}");

    println!("[COVENANT P2SH-WS] Attempting invalid spend (increase by 2)");
    let bad_tx = build_spend_tx(&state, state.counter + 2, &covenant_script);
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
}

impl CovenantState {
    fn new(counter: u8, covenant_script: &[u8]) -> Self {
        let tx = genesis_tx(counter, covenant_script);
        Self::from_tx(tx, covenant_script, counter)
    }

    fn from_tx(tx: Transaction, covenant_script: &[u8], counter: u8) -> Self {
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let spk = build_spk(counter, covenant_script);
        let utxo_entry = UtxoEntry::new(1_000_000, spk, 0, false);
        Self { utxo_outpoint: outpoint, utxo_entry, counter }
    }
}

/// Build the covenant script that enforces:
/// 1) The counter is incremented (state -> state+1).
/// 2) The spend has exactly one output.
/// 3) The output script public key matches the same redeem script and embeds the hash of (state+1) in the state slot.
fn build_covenant_script() -> ScriptBuilderResult<Vec<u8>> {
    Ok(ScriptBuilder::new()
			// Compute next_state = state + 1 and keep it on stack for later hashing
			.add_op(Op1Add)?
			.add_op(OpDup)?
			// Enforce single-output spend
			.add_op(OpTxOutputCount)?
			.add_i64(1)?
			.add_op(OpEqualVerify)?
			// Enforce output[0] serialized SPK length matches input length
			.add_i64(0)?
			.add_op(OpTxOutputSpkLen)?
			.add_i64(0)?
			.add_op(OpTxInputSpkLen)?
			.add_op(OpEqualVerify)?
			// Compare opcode+redeemScriptHash prefix (script bytes 0..37)
			// Output prefix
			.add_i64(0)? // idx
			.add_i64(0)?.add_op(OpTxOutputSpkLen)? // total len
			.add_i64(70)?.add_op(OpSub)? // header = total - 70
			.add_op(OpDup)? // header, header
			.add_i64(37)?.add_op(OpAdd)? // end = header + 37
			.add_op(OpTxOutputSpkSubstr)?
			// Input prefix
			.add_i64(0)? // idx
			.add_i64(0)?.add_op(OpTxInputSpkLen)?
			.add_i64(70)?.add_op(OpSub)?
			.add_op(OpDup)?
			.add_i64(37)?.add_op(OpAdd)?
			.add_op(OpTxInputSpkSubstr)?
			.add_op(OpEqualVerify)?
			// Enforce final opcode of output[0] script is OP_EQUAL (script byte 69)
			.add_i64(0)?
			.add_i64(0)?.add_op(OpTxOutputSpkLen)?
			.add_i64(70)?.add_op(OpSub)? // header
			.add_i64(69)?.add_op(OpAdd)? // start = header + 69
			.add_op(OpDup)?
			.add_i64(1)?.add_op(OpAdd)? // end = start + 1
			.add_op(OpTxOutputSpkSubstr)?
			.add_data(&[OpEqual])?
			.add_op(OpEqualVerify)?
			// Hash next_state and require it matches the state slot in the output script public key (script bytes 37..69)
			.add_i64(0)?
			.add_i64(0)?.add_op(OpTxOutputSpkLen)?
			.add_i64(70)?.add_op(OpSub)? // header
			.add_op(OpDup)?
			.add_i64(37)?.add_op(OpAdd)? // start = header + 37
			.add_op(OpSwap)? // bring start above header
			.add_op(OpDrop)? // drop header
			.add_op(OpDup)?
			.add_i64(32)?.add_op(OpAdd)? // end = start + 32
			.add_op(OpTxOutputSpkSubstr)? // pushes state hash from SPK
			.add_op(OpSwap)?
			.add_op(OpBlake2b)? // hash(next_state)
			.add_op(OpEqualVerify)?
			// Clean stack and succeed
			.add_op(OpDrop)?
			.add_op(OpTrue)?
			.drain())
}

/// Build the spend transaction for the next counter value.
fn build_spend_tx(state: &CovenantState, next_counter: u8, covenant_script: &[u8]) -> Transaction {
    let sig_script = ScriptBuilder::new()
		.add_data(&encode_counter(state.counter))
		.unwrap()
		// For P2SH-with-state the redeem script must be the last stack item in the signature script
		.add_data(covenant_script)
		.unwrap()
		.drain();

    let input = TransactionInput::new(state.utxo_outpoint, sig_script, 0, 0);
    let output = TransactionOutput::new(state.utxo_entry.amount, build_spk(next_counter, covenant_script));

    let mut tx = Transaction::new(0, vec![input], vec![output], 0, SubnetworkId::default(), 0, vec![]);
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
    let mut vm = TxScriptEngine::from_transaction_input(&populated, &tx.inputs[0], 0, utxo_entry, reused_values, sig_cache, flags);
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
    let state = encode_counter(counter);
    ScriptPublicKey::new(0, pay_to_script_hash_with_state(&state, covenant_script))
}

fn encode_counter(counter: u8) -> Vec<u8> {
    if counter == 0 {
        vec![]
    } else {
        vec![counter]
    }
}
