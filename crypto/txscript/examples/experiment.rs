use kaspa_consensus_core::constants::{SOMPI_PER_KASPA, TX_VERSION};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::{opcodes::codes::*, pay_to_script_hash_script, script_builder::ScriptBuilder, EngineFlags, TxScriptEngine};

// -------------------------
// MOCK TRANSACTION CREATOR
// -------------------------
fn make_mock_transaction(script: &[u8]) -> (Transaction, UtxoEntry) {
    let spk = pay_to_script_hash_script(&script);

    let dummy_prev_out = TransactionOutpoint::new(TransactionId::from_bytes([0u8; 32]), 0);
    let dummy_input = TransactionInput::new(dummy_prev_out.clone(), vec![], 0, 0);
    let dummy_output = TransactionOutput::new(SOMPI_PER_KASPA, spk.clone());

    let tx = Transaction::new(TX_VERSION, vec![dummy_input.clone()], vec![dummy_output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

    let utxo_entry = UtxoEntry::new(SOMPI_PER_KASPA, spk, 0, false);

    (tx, utxo_entry)
}

// -------------------------
// REDEEM SCRIPT BUILDER
// -------------------------
fn build_redeem_script(cur_input: i64, end: i64, data: &[u8]) -> Vec<u8> {
    ScriptBuilder::new()
        // move new state to alt stack
        .add_op(OpToAltStack).unwrap() // todo must be part of new state, currently just move to alt stack

        // embedded current state
        .add_op(OpFalse).unwrap()
        .add_op(OpIf).unwrap()
        .add_data(data).unwrap()
        .add_op(OpEndIf).unwrap()

        .add_i64(cur_input).unwrap()
        .add_i64((data.len() + 2) as i64).unwrap()// 2x OpPushDataX + len of data
        .add_i64(end).unwrap()
        .add_op(OpTxInputScriptSigSubStr).unwrap()
        // Duplicate and hash the extracted redeem script
        .add_op(OpDup).unwrap()
        .add_op(OpBlake2b).unwrap()
        // Build expected SPK: version + OpBlake2b + OpData32 + hash + OpEqual
        .add_data(&TX_VERSION.to_le_bytes()).unwrap()
        .add_data(&[OpBlake2b]).unwrap()
        .add_op(OpCat).unwrap()
        .add_data(&[OpData32]).unwrap()
        .add_op(OpCat).unwrap()
        .add_op(OpSwap).unwrap()
        .add_op(OpCat).unwrap()
        .add_data(&[OpEqual]).unwrap()
        .add_op(OpCat).unwrap()
        // Compare with input SPK
        .add_op(OpDup).unwrap()
        .add_i64(0).unwrap()
        .add_op(OpTxInputSpk).unwrap()
        .add_op(OpEqualVerify).unwrap()
        // Compare with output SPK
        .add_i64(0).unwrap()
        .add_op(OpTxOutputSpk).unwrap()
        .add_op(OpEqualVerify).unwrap()
        .drain()
}

// -------------------------
// MAIN
// -------------------------
fn main() {
    let cur_input = 0i64;
    // Step 1: compute redeem script length with a placeholder end
    let placeholder_end = 17i64;
    let data = b"somedata";
    let computed_len = build_redeem_script(cur_input, placeholder_end, data).len() as i64;

    let end = 2 + data.len() as i64 + computed_len;

    // Step 2: build the actual redeem script with the correct end
    let redeem_script = build_redeem_script(cur_input, end, data);

    // Step 3: make mock transaction
    let (mut tx, utxo_entry) = make_mock_transaction(&redeem_script);

    // Step 4: set redeem script into signature_script
    tx.inputs[0].signature_script = ScriptBuilder::new().add_data(data).unwrap().add_data(&redeem_script).unwrap().drain();
    // Step 5: execute TxScriptEngine to verify
    let sig_cache = Cache::new(10_000);
    let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let mut engine = TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, &reused_values, &sig_cache, flags);

    match engine.execute() {
        Ok(_) => println!("Script execution succeeded with checks!"),
        Err(e) => println!("Script execution failed: {:?}", e),
    }
}
