use std::time::Instant;
use kaspa_consensus_core::constants::{SOMPI_PER_KASPA, TX_VERSION};
use kaspa_consensus_core::hashing::sighash::SigHashReusedValuesUnsync;
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{
    PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry,
};
use kaspa_txscript::caches::Cache;
use kaspa_txscript::opcodes::codes::{
    OpAdd, OpBlake2b, OpCat, OpData32, OpData8, OpDup, OpEqual, OpEqualVerify, OpSHA256, OpSwap,
    OpTxInputIndex, OpTxInputScriptSigLen, OpTxInputScriptSigSubStr, OpTxOutputSpk, OpTxPayloadSubstr, OpZkPrecompile,
};
use kaspa_txscript::zk_precompiles::tags::ZkTag;
use kaspa_txscript::{
     pay_to_script_hash_script, script_builder::ScriptBuilder, EngineFlags, SpkEncoding, TxScriptEngine,
};
use risc0_zkvm::sha::Digestible;
use risc0_zkvm::{default_prover, ExecutorEnv, Prover, ProverOpts};
use zk_covenant_inline_core::PublicInput;
use zk_covenant_inline_methods::{ZK_COVENANT_INLINE_GUEST_ELF, ZK_COVENANT_INLINE_GUEST_ID};

fn main() {
    // Initialize tracing. In order to view logs, run `RUST_LOG=info cargo run`
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env()).init();

    // --- Build the transaction for the guest and for verification ---
    let public_input = PublicInput { payload_diff: 256, prev_state: 128, new_state: 384 };
    let journal = bytemuck::bytes_of(&public_input);
    println!("Journal: {}", faster_hex::hex_string(journal));
    println!("new state: {}", faster_hex::hex_string(&public_input.new_state.to_le_bytes()));
    println!("payload_diff: {}", faster_hex::hex_string(&public_input.payload_diff.to_le_bytes()));
    println!("prev_state: {}", faster_hex::hex_string(&public_input.prev_state.to_le_bytes()));

    let new_state_bytes = &public_input.new_state.to_le_bytes();
    let expected_digest = journal.digest();
    let computed_len = build_redeem_script(public_input.prev_state, 20).len() as i64;

    let input_redeem_script = build_redeem_script(public_input.prev_state, computed_len);
    let input_spk = pay_to_script_hash_script(&input_redeem_script);
    let output_redeem_script = build_redeem_script(public_input.new_state, computed_len);
    let output_spk = pay_to_script_hash_script(&output_redeem_script);
    assert_eq!(computed_len, output_redeem_script.len() as i64);
    assert_eq!(computed_len, input_redeem_script.len() as i64);

    println!("Output redeem script: {}", faster_hex::hex_string(&output_redeem_script));
    println!("output spk: {}", faster_hex::hex_string(&output_spk.to_bytes()));

    let (mut tx, _, utxo_entry) = make_mock_transaction(0, input_spk, output_spk, public_input.payload_diff);

    let env = ExecutorEnv::builder().write_slice(bytemuck::bytes_of(&public_input)).build().unwrap();

    // Obtain the default prover.
    let prover = default_prover();

    let now = Instant::now();
    // Proof information by proving the specified ELF binary.
    // This struct contains the receipt along with statistics about execution of the guest
    let prove_info = prover.prove_with_opts(env, ZK_COVENANT_INLINE_GUEST_ELF, &ProverOpts::succinct()).unwrap();
    println!("Proving took {} ms", now.elapsed().as_millis());

    // extract the receipt.
    let receipt = prove_info.receipt;
    let receipt_inner = receipt.inner.succinct().unwrap();

    // The guest commits the txid of the transaction it validated.
    // We assert that it matches the txid we calculated.
    let output: &PublicInput = bytemuck::from_bytes(receipt.journal.bytes.as_slice());
    assert_eq!(output, &public_input);

    let script_precompile_inner = {
        use kaspa_txscript::zk_precompiles::risc0::inner::Inner;
        use kaspa_txscript::zk_precompiles::risc0::merkle::MerkleProof;
        Inner {
            seal: receipt_inner.seal.clone(),
            control_id: receipt_inner.control_id,
            claim: receipt_inner.claim.digest(),
            hashfn: receipt_inner.hashfn.clone(),
            verifier_parameters: receipt_inner.verifier_parameters,
            control_inclusion_proof: MerkleProof::new(
                receipt_inner.control_inclusion_proof.index,
                receipt_inner.control_inclusion_proof.digests.clone(),
            ),
        }
    };
    let journal_digest = receipt.journal.digest();
    assert_eq!(journal_digest, expected_digest);
    // The receipt was verified at the end of proving, but the below code is an
    // example of how someone else could verify this receipt.
    receipt.verify(ZK_COVENANT_INLINE_GUEST_ID).unwrap();

    // --- Now, we update the sig_script with the real proof and verify on-chain ---
    let final_sig_script = ScriptBuilder::new()
        .add_data(&borsh::to_vec(&script_precompile_inner).unwrap())
        .unwrap()
        .add_data(bytemuck::cast_slice(ZK_COVENANT_INLINE_GUEST_ID.as_slice()))
        .unwrap()
        .add_data(new_state_bytes)
        .unwrap()
        .add_data(&input_redeem_script)
        .unwrap()
        .drain();

    tx.inputs[0].signature_script = final_sig_script;

    verify_zk_succinct(&tx, &utxo_entry);
    println!("ZK proof verified successfully on-chain!");
}

fn make_mock_transaction(
    lock_time: u64,
    input_spk: ScriptPublicKey,
    output_spk: ScriptPublicKey,
    payload_diff: u64,
) -> (Transaction, TransactionInput, UtxoEntry) {
    let dummy_prev_out = TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 1);
    let dummy_tx_input = TransactionInput::new(dummy_prev_out, vec![], 10, u8::MAX);

    let dummy_tx_out = TransactionOutput::new(SOMPI_PER_KASPA, output_spk);

    let tx = Transaction::new(
        TX_VERSION + 1,
        vec![dummy_tx_input.clone()],
        vec![dummy_tx_out.clone()],
        lock_time,
        SUBNETWORK_ID_NATIVE,
        0,
        payload_diff.to_le_bytes().to_vec(),
    );
    let utxo_entry = UtxoEntry::new(0, input_spk, 0, false);
    (tx, dummy_tx_input, utxo_entry)
}

fn verify_zk_succinct(tx: &Transaction, utxo_entry: &UtxoEntry) {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, vec![utxo_entry.clone()]);
    let mut vm = TxScriptEngine::from_transaction_input(&populated, &tx.inputs[0], 0, utxo_entry, &reused_values, &sig_cache, flags);
    vm.execute().unwrap();
}

fn build_redeem_script(state: u64, redeem_script_len: i64) -> Vec<u8> {
    ScriptBuilder::new()
        .add_data(&state.to_le_bytes()).unwrap()
        .add_op(OpSwap).unwrap()
        .add_op(OpDup).unwrap()

        // verify new state is preimage of output spk
        .add_data(&[OpData8]).unwrap()// op data for state of 8 bytes
        .add_op(OpSwap).unwrap()

        // pushdata8, state of 8 bytes
        .add_op(OpCat).unwrap()

        // offset
        .add_op(OpTxInputIndex).unwrap()
            .add_op(OpTxInputIndex).unwrap()
            .add_op(OpTxInputScriptSigLen).unwrap()
        .add_i64({
            -redeem_script_len
                + 1 + size_of::<u64>() as i64 // data + OpPushData
        }).unwrap()
        .add_op(OpAdd).unwrap()

         // end
        .add_op(OpTxInputIndex).unwrap()
        .add_op(OpTxInputScriptSigLen).unwrap()
        .add_op(OpTxInputScriptSigSubStr).unwrap()
        // pushdata8, state of 8 bytes, redeem script suffix, so we have redeem script here
        .add_op(OpCat).unwrap()

        // hash redeem script
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

        // verify new state + suffix of redeem script is used as preimage for output spk
        .add_op(OpTxInputIndex).unwrap()
        .add_op(OpTxOutputSpk).unwrap()
        .add_op(OpEqualVerify).unwrap()

        // concatenate new state and old state
        .add_op(OpCat).unwrap()
        .add_i64(0).unwrap() // start
        .add_i64(size_of::<u64>() as i64).unwrap() // end of 8 byte state
        .add_op(OpTxPayloadSubstr).unwrap()
        // concat payload diff and states
        .add_op(OpCat).unwrap()
        // hash preimage
        .add_op(OpSHA256).unwrap()
        // swap journal with program id
        .add_op(OpSwap).unwrap()

        // zk verification
        .add_data(&[ZkTag::R0Succinct as u8])
        .unwrap()
        .add_op(OpZkPrecompile).unwrap()

        // verify current input index is 0
        .add_op(OpTxInputIndex).unwrap()
        .add_i64(0).unwrap()
        .add_op(OpEqualVerify).unwrap()
        .drain()
}
