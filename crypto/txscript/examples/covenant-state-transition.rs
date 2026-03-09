use kaspa_consensus_core::constants::{SOMPI_PER_KASPA, TX_VERSION};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::engine_context::EngineContext;
use kaspa_txscript::{EngineFlags, TxScriptEngine, opcodes::codes::*, pay_to_script_hash_script, script_builder::ScriptBuilder};

// -------------------------
// MOCK TRANSACTION CREATOR
// -------------------------
fn make_mock_transaction(input_script: &[u8], output_script: &[u8]) -> (Transaction, UtxoEntry) {
    let input_spk = pay_to_script_hash_script(input_script);
    let output_spk = pay_to_script_hash_script(output_script);

    let dummy_prev_out = TransactionOutpoint::new(TransactionId::from_bytes([0u8; 32]), 0);
    let dummy_input = TransactionInput::new(dummy_prev_out, vec![], 0, 0);
    let dummy_output = TransactionOutput::new(SOMPI_PER_KASPA, output_spk);

    let tx = Transaction::new(TX_VERSION, vec![dummy_input.clone()], vec![dummy_output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

    let utxo_entry = UtxoEntry::new(SOMPI_PER_KASPA, input_spk, 0, false, None);

    (tx, utxo_entry)
}

// -------------------------
// REDEEM SCRIPT BUILDER
// -------------------------
fn build_redeem_script(end: i64, state: &[u8]) -> Vec<u8> {
    let op_to_add_state = {
        let script = ScriptBuilder::new().add_data(state).unwrap().drain();
        script[0]
    };
    ScriptBuilder::new()
        // embedded current state
        .add_op(OpFalse).unwrap()
        .add_op(OpIf).unwrap()
        .add_data(state).unwrap()
        .add_op(OpEndIf).unwrap()

        // verify expected len, it guarantees that script sig only contains OpPushData, state, OpPushData, redeem script
        .add_op(OpTxInputIndex).unwrap()
        .add_op(OpTxInputScriptSigLen).unwrap()
        .add_i64(end).unwrap()
        .add_op(OpEqualVerify).unwrap()

        // prefix of the script
        .add_data(&[OpFalse, OpIf, op_to_add_state]).unwrap()
        .add_op(OpSwap).unwrap()
        // + new state
        .add_op(OpCat).unwrap()
        // + suffix of the script
        .add_op(OpTxInputIndex).unwrap()
        .add_i64({
            2 + state.len() // new state + OpPushDataX * 2
            + 2 // OpFalse + OpIf
            + 1 + state.len() // OpPushDataX + data
        } as i64).unwrap()//  + len of data
        .add_i64(end).unwrap()
        .add_op(OpTxInputScriptSigSubstr).unwrap()
        .add_op(OpCat).unwrap()

        // Duplicate and hash the extracted redeem script
        .add_op(OpDup).unwrap()
        .add_op(OpBlake2b).unwrap()

        // Build expected SPK: version + OpBlake2b + OpData32 + hash + OpEqual
        .add_data(&{
            let mut data = [0u8; 4];
            data[0..2].copy_from_slice(&TX_VERSION.to_le_bytes());
            data[2] = OpBlake2b;
            data[3] = OpData32;
            data
        }).unwrap()
        // swap hash and prefix of spk
        .add_op(OpSwap).unwrap()
        // version + OpBlake2b + OpData32 + hash
        .add_op(OpCat).unwrap()
        .add_data(&[OpEqual]).unwrap()
        // output spk is ready
        .add_op(OpCat).unwrap()

        // Compare with output SPK
        .add_op(OpTxInputIndex).unwrap()
        .add_op(OpTxOutputSpk).unwrap()
        .add_op(OpEqualVerify).unwrap()
        .drain()
}

// -------------------------
// MAIN
// -------------------------
fn main() {
    // Step 1: compute redeem script length with a placeholder end
    let placeholder_end = 17i64;
    let input_data = b"somedata";
    let output_data = b"new data";

    assert_eq!(input_data.len(), output_data.len());
    // println!("data as hex: {}", hex::encode(data));

    let computed_len = build_redeem_script(placeholder_end, input_data).len() as i64;

    let end = 2 + input_data.len() as i64 + computed_len;

    // Step 2: build the actual redeem script with the correct end
    let input_redeem_script = build_redeem_script(end, input_data);
    let output_redeem_script = build_redeem_script(end, output_data);
    assert_eq!(input_redeem_script.len() as i64, computed_len);
    assert_eq!(output_redeem_script.len() as i64, computed_len);

    // println!("Redeem script: {}", hex::encode(&redeem_script));

    // Step 3: make mock transaction
    let (mut tx, utxo_entry) = make_mock_transaction(&input_redeem_script, &output_redeem_script);

    // Step 4: set redeem script into signature_script
    tx.inputs[0].signature_script =
        ScriptBuilder::new().add_data(output_data).unwrap().add_data(&input_redeem_script).unwrap().drain();
    // Step 5: execute TxScriptEngine to verify
    let sig_cache = Cache::new(10_000);
    let tx = PopulatedTransaction::new(&tx, vec![utxo_entry.clone()]);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let ctx = EngineContext::new(&sig_cache).with_reused(&reused_values);
    let mut engine = TxScriptEngine::from_transaction_input(&tx, &tx.tx.inputs[0], 0, &utxo_entry, ctx, flags);

    match engine.execute() {
        Ok(_) => println!("Script execution succeeded with checks!"),
        Err(e) => println!("Script execution failed: {:?}", e),
    }
}
