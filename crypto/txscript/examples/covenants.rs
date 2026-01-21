use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::hashing::tx::transaction_id_preimage;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_hashes::Hash;
use kaspa_txscript::caches::Cache;
use kaspa_txscript::opcodes::codes::{
    Op1Add, OpBlake2bWithKey, OpCat, OpDup, OpEqual, OpEqualVerify, OpOutpointTxId, OpRot, OpTxInputIndex, OpTxInputSpk,
    OpTxOutputCount, OpTxOutputSpk, OpTxPayloadLen, OpTxPayloadSubstr,
};
use kaspa_txscript::script_builder::{ScriptBuilder, ScriptBuilderResult};
use kaspa_txscript::{pay_to_script_hash_script, EngineCtx};
use kaspa_txscript::{EngineFlags, TxScriptEngine};
use kaspa_txscript_errors::TxScriptError;

fn main() -> ScriptBuilderResult<()> {
    counter_example()
}

/// Demonstrates a simple covenant that enforces a counter stored in the transaction payload.
/// Each spend must increment the counter and return funds to the same script public key.
/// A spend that does not increment the counter is rejected.
fn counter_example() -> ScriptBuilderResult<()> {
    println!("[COVENANT] Counter payload covenant");
    let covenant_script = build_covenant_script()?;
    let spk = pay_to_script_hash_script(&covenant_script);

    // Shared engine state
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    // Create the initial UTXO with counter = 0
    let mut state = CovenantState::new(0, &spk);

    // Two valid increments
    for next in [1u8, 2u8] {
        println!("[COVENANT] Spending to counter {next}");
        let tx = build_spend_tx(&state, next, &spk, &covenant_script);
        run_vm(&tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect("covenant spend should succeed");
        state = CovenantState::from_tx(tx, &spk, next);
    }

    let counter_2_state = state.clone();
    let next = 3u8;
    println!("[COVENANT] Spending to counter {next}");
    let tx = build_spend_tx(&state, next, &spk, &covenant_script);
    run_vm(&tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect("covenant spend should succeed");
    state = CovenantState::from_tx(tx, &spk, next);

    println!("[COVENANT] Attempting invalid spend (no increment)");
    let bad_tx = build_spend_tx(&state, state.counter, &spk, &covenant_script);
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT] Expected failure: {err:?}");

    println!("[COVENANT] Attempting invalid spend (no increment and reuse previous state)");
    // We try to spend the last UTXO but provide the previous state with counter=2
    let bad_tx = build_spend_tx(
        &CovenantState { utxo_outpoint: state.utxo_outpoint, ..counter_2_state },
        state.counter,
        &spk,
        &covenant_script,
    );
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT] Expected failure: {err:?}");

    println!("[COVENANT] Attempting invalid spend (increase by 2)");
    let bad_tx = build_spend_tx(&state, state.counter + 2, &spk, &covenant_script);
    let err = run_vm(&bad_tx, &state.utxo_entry, &sig_cache, &reused_values, flags).expect_err("non-incrementing spend must fail");
    println!("[COVENANT] Expected failure: {err:?}");

    println!("[COVENANT] Example complete");
    Ok(())
}

/// Holds the current covenant UTXO state.
#[derive(Clone)]
struct CovenantState {
    prev_tx_rest: Vec<u8>,
    prev_payload: Vec<u8>,
    utxo_outpoint: TransactionOutpoint,
    utxo_entry: UtxoEntry,
    counter: u8,
}

impl CovenantState {
    fn new(counter: u8, spk: &kaspa_consensus_core::tx::ScriptPublicKey) -> Self {
        let payload = encode_counter(counter);
        let tx = genesis_tx(&payload, spk.clone());
        Self::from_tx(tx, spk, counter)
    }

    fn from_tx(tx: Transaction, spk: &kaspa_consensus_core::tx::ScriptPublicKey, counter: u8) -> Self {
        let preimage = transaction_id_preimage(&tx);
        let payload_len = tx.payload.len();
        let (rest, payload) = preimage.split_at(preimage.len() - payload_len);
        let outpoint = TransactionOutpoint::new(tx.id(), 0);
        let utxo_entry = UtxoEntry::new(1_000_000, spk.clone(), 0, false, None);
        Self { prev_tx_rest: rest.to_vec(), prev_payload: payload.to_vec(), utxo_outpoint: outpoint, utxo_entry, counter }
    }
}

/// Build the covenant script described in the docs.
fn build_covenant_script() -> ScriptBuilderResult<Vec<u8>> {
    Ok(ScriptBuilder::new()
			// Hash(prev_tx_rest || prev_tx_payload) with domain "TransactionID" and verify matches input outpoint txid
			.add_op(OpDup)?
			.add_op(OpRot)?
			.add_op(OpRot)?
			.add_op(OpCat)?
			.add_data(b"TransactionID")?
			.add_op(OpBlake2bWithKey)?
			.add_op(OpTxInputIndex)?
			.add_op(OpOutpointTxId)?
			.add_op(OpEqualVerify)?
			// Enforce payload increment: payload_of_tx == prev_payload + 1
			.add_op(Op1Add)?
			.add_i64(0)?
			.add_op(OpTxPayloadLen)?
			.add_op(OpTxPayloadSubstr)?
			.add_op(OpEqualVerify)?
			// Enforce same script pub key and single-output spend
			.add_op(OpTxInputIndex)?
			.add_op(OpTxInputSpk)?
			.add_i64(0)?
			.add_op(OpTxOutputSpk)?
			.add_op(OpEqualVerify)?
			.add_op(OpTxOutputCount)?
			.add_i64(1)?
			.add_op(OpEqual)?
			.drain())
}

/// Build the spend transaction for the next counter value.
fn build_spend_tx(
    state: &CovenantState,
    next_counter: u8,
    spk: &kaspa_consensus_core::tx::ScriptPublicKey,
    covenant_script: &[u8],
) -> Transaction {
    let payload = encode_counter(next_counter);
    let sig_script = ScriptBuilder::new()
		.add_data(&state.prev_tx_rest)
		.unwrap()
		.add_data(&state.prev_payload)
		.unwrap()
		// For P2SH the redeem script must be the last stack item in the signature script
		.add_data(covenant_script)
		.unwrap()
		.drain();

    let input = TransactionInput::new(state.utxo_outpoint, sig_script, 0, 0);
    let output = TransactionOutput::new(state.utxo_entry.amount, spk.clone());

    let mut tx = Transaction::new(0, vec![input], vec![output], 0, SubnetworkId::default(), 0, payload);
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
fn genesis_tx(payload: &[u8], spk: kaspa_consensus_core::tx::ScriptPublicKey) -> Transaction {
    let dummy_input = TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(0), 0), vec![], 0, 0);
    let output = TransactionOutput::new(1_000_000, spk);
    let mut tx = Transaction::new(0, vec![dummy_input], vec![output], 0, SubnetworkId::default(), 0, payload.to_vec());
    tx.finalize();
    tx
}

fn encode_counter(counter: u8) -> Vec<u8> {
    if counter == 0 {
        vec![]
    } else {
        vec![counter]
    }
}
