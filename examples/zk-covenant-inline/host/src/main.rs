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

    // The guest commits the public input of the guest program args it validated.
    // We assert that it matches the output we calculated.
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

fn build_redeem_script(old_state: u64, redeem_script_len: i64) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // Prepare old and new states on the stack
    add_state_preparation(&mut builder, old_state).unwrap();

    // Stack: [proof, program_id, old_state, new_state, new_state]

    // Build the prefix for the new redeem script (OpData8 || new_state)
    add_new_redeem_prefix(&mut builder).unwrap();

    // Stack: [proof, program_id, old_state, new_state, (OpData8 || new_state)]

    // Extract the suffix from sig_script and concatenate to form the new redeem script
    add_suffix_extraction_and_cat(&mut builder, redeem_script_len).unwrap();

    // Stack: [proof, program_id, old_state, new_state, new_redeem_script]

    // Hash the new redeem script and build the expected SPK bytes
    add_hash_and_build_spk(&mut builder).unwrap();

    // Stack: [proof, program_id, old_state, new_state, constructed_spk]

    // Verify the constructed SPK matches the actual output SPK
    add_verify_output_spk(&mut builder).unwrap();

    // Stack: [proof, program_id, old_state, new_state]

    // Construct the preimage for the journal hash (old_state || new_state || payload_diff)
    add_construct_journal_preimage(&mut builder).unwrap();

    // Stack: [proof, program_id, preimage]

    // Hash the preimage to get the journal hash
    add_hash_to_journal(&mut builder).unwrap();

    // Stack: [proof, program_id, journal_hash]

    // Swap journal_hash and program_id for ZK verification order
    add_swap_for_zk(&mut builder).unwrap();

    // Stack: [proof, journal_hash, program_id]

    // Perform ZK verification using the proof, journal_hash, program_id, and tag
    add_zk_verification(&mut builder).unwrap();

    // Stack: [] (assuming OpZkPrecompile consumes the items and leaves nothing or true; but since it verifies, likely leaves nothing)

    // Verify that the current input index is 0 (ensuring single-input tx or specific input)
    add_verify_input_index_zero(&mut builder).unwrap();

    // Stack: [] (OpEqualVerify consumes and verifies)

    builder.drain()
}

/// Prepares the old and new states on the stack by pushing the old state, swapping, and duplicating the new state.
///
/// Expects on stack: [proof, program_id, new_state]
///
/// Leaves on stack: [proof, program_id, old_state, new_state, new_state]
fn add_state_preparation(builder: &mut ScriptBuilder, old_state: u64) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push the old state (prev_state) onto the stack
    builder.add_data(&old_state.to_le_bytes())?;
    // Swap the top two items: new_state and old_state
    builder.add_op(OpSwap)?;
    // Duplicate the new_state (now on top)
    builder.add_op(OpDup)
}

/// Builds the prefix for the new redeem script by concatenating OpData8 with a duplicated new_state.
///
/// Expects on stack: [..., old_state, new_state, new_state]
///
/// Leaves on stack: [..., old_state, new_state, (OpData8 || new_state)]
fn add_new_redeem_prefix(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push OpData8 (for pushing 8-byte state in the new redeem script)
    builder.add_data(&[OpData8])?;
    // Swap to bring one new_state to top
    builder.add_op(OpSwap)?;
    // Concatenate: OpData8 || new_state
    builder.add_op(OpCat)
}

/// Extracts the redeem script suffix from the signature script and concatenates it with the prefix to form the new redeem script.
///
/// The suffix is extracted using a computed offset to skip the old state push in the current redeem script.
///
/// Expects on stack: [..., (OpData8 || new_state)] (prefix on top)
///
/// Leaves on stack: [..., new_redeem_script]
fn add_suffix_extraction_and_cat(builder: &mut ScriptBuilder, redeem_script_len: i64) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Compute offset: sig_script_len + (-redeem_script_len + 1 + 8) to point after old_state push
    builder.add_op(OpTxInputIndex)?;
    builder.add_op(OpTxInputIndex)?;
    builder.add_op(OpTxInputScriptSigLen)?;
    builder.add_i64(-redeem_script_len + 1 + std::mem::size_of::<u64>() as i64)?; // -redeem script + OpPushDataX + state len
    builder.add_op(OpAdd)?;

    // Push end: sig_script_len
    builder.add_op(OpTxInputIndex)?;
    builder.add_op(OpTxInputScriptSigLen)?;

    // Extract substring: sig_script[offset..end] = suffix
    builder.add_op(OpTxInputScriptSigSubStr)?;

    // Concatenate prefix || suffix to form new_redeem_script
    builder.add_op(OpCat)
}

/// Hashes the new redeem script and constructs the expected ScriptPublicKey (SPK) bytes for verification.
///
/// The SPK is built as: version (2 bytes) || OpBlake2b || OpData32 || hash || OpEqual
///
/// Expects on stack: [..., new_redeem_script]
///
/// Leaves on stack: [..., constructed_spk]
fn add_hash_and_build_spk(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Hash the new redeem script using Blake2b
    builder.add_op(OpBlake2b)?;

    // Push prefix: version (little-endian) || OpBlake2b || OpData32
    let mut data = [0u8; 4];
    data[0..2].copy_from_slice(&TX_VERSION.to_le_bytes());
    data[2] = OpBlake2b;
    data[3] = OpData32;
    builder.add_data(&data)?;

    // Swap to bring hash to top
    builder.add_op(OpSwap)?;

    // Concatenate: prefix || hash
    builder.add_op(OpCat)?;

    // Push OpEqual
    builder.add_data(&[OpEqual])?;

    // Concatenate: (prefix || hash) || OpEqual = constructed_spk
    builder.add_op(OpCat)
}

/// Verifies that the constructed SPK matches the actual output SPK at index 0.
///
/// Expects on stack: [..., constructed_spk]
///
/// Leaves on stack: [...] (consumes constructed_spk and output_spk after verification)
fn add_verify_output_spk(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push input index (0)
    builder.add_op(OpTxInputIndex)?;

    // Push output SPK at index 0
    builder.add_op(OpTxOutputSpk)?;

    // Verify equality and consume both items
    builder.add_op(OpEqualVerify)
}

/// Constructs the preimage for the journal hash by concatenating old_state || new_state || payload_diff (from tx payload substr).
///
/// Expects on stack: [..., old_state, new_state]
///
/// Leaves on stack: [..., preimage] where preimage = old_state || new_state || payload_diff
fn add_construct_journal_preimage(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Concatenate old_state || new_state
    builder.add_op(OpCat)?;

    // Push start (0) and end (8) for payload substr
    builder.add_i64(0)?;
    builder.add_i64(std::mem::size_of::<u64>() as i64)?;

    // Extract payload_diff = tx.payload[0..8]
    builder.add_op(OpTxPayloadSubstr)?;

    // Concatenate (old_state || new_state) || payload_diff
    builder.add_op(OpCat)
}

/// Hashes the preimage to compute the journal hash using SHA256.
///
/// Expects on stack: [..., preimage]
///
/// Leaves on stack: [..., journal_hash]
fn add_hash_to_journal(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Hash the preimage with SHA256 to get the journal commitment
    builder.add_op(OpSHA256)
}

/// Swaps the journal_hash and program_id to prepare the stack for ZK verification.
///
/// Expects on stack: [..., program_id, journal_hash]
///
/// Leaves on stack: [..., journal_hash, program_id]
fn add_swap_for_zk(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Swap the top two items
    builder.add_op(OpSwap)
}

/// Adds the ZK tag and opcode to perform the ZK precompile verification.
///
/// Assumes OpZkPrecompile consumes [proof (bottom), journal_hash, program_id, tag (top)] and verifies the proof.
///
/// Expects on stack: [proof, journal_hash, program_id]
///
/// Leaves on stack: [] (after verification; assumes success leaves nothing or implicit true)
fn add_zk_verification(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push the ZK tag for RISC0 succinct proof
    builder.add_data(&[ZkTag::R0Succinct as u8])?;

    // Execute the ZK precompile opcode to verify the proof
    builder.add_op(OpZkPrecompile)
}

/// Verifies that the current input index is 0 (e.g., to enforce single-input or specific input constraints).
///
/// Expects on stack: []
///
/// Leaves on stack: [] (after verification)
fn add_verify_input_index_zero(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push current input index
    builder.add_op(OpTxInputIndex)?;

    // Push 0
    builder.add_i64(0)?;

    // Verify equality
    builder.add_op(OpEqualVerify)
}