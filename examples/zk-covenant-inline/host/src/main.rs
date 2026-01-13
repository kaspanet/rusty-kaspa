use std::time::Instant;
use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    hashing::sighash::SigHashReusedValuesUnsync,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_txscript::{
    caches::Cache,
    opcodes::codes::{
        OpAdd, OpBlake2b, OpCat, OpData32,  OpDup, OpEqual, OpEqualVerify, OpSHA256, OpSwap,
    OpTxInputIndex, OpTxInputScriptSigLen, OpTxInputScriptSigSubStr, OpTxOutputSpk, OpTxPayloadSubstr, OpZkPrecompile,},
    pay_to_script_hash_script,
    script_builder::ScriptBuilder,
    zk_precompiles::{risc0::merkle::MerkleProof, risc0::rcpt::SuccinctReceipt, tags::ZkTag},
    EngineFlags, TxScriptEngine,
};
use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv, Prover, ProverOpts};
use std::time::Instant;
use zk_covenant_inline_core::{Action, PublicInput, State, VersionedActionRaw};
use zk_covenant_inline_methods::{ZK_COVENANT_INLINE_GUEST_ELF, ZK_COVENANT_INLINE_GUEST_ID};

fn main() {
    // Initialize tracing. In order to view logs, run `RUST_LOG=info cargo run`
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env()).init();

    // --- Build the transaction for the guest and for verification ---
    let state = State::default();
    let prev_state_hash = state.hash();
    let action = Action::Fib(5); // Example action
    let new_state = {
        let mut state = State::default();
        state.add_new_result(action, 5);
        state
    };

    let (action_disc, action_value) = action.split();
    let action_bytes = [action_disc, action_value];
    let public_input =
        PublicInput { prev_state_hash, versioned_action_raw: VersionedActionRaw { action_version: 0, action_raw: action_bytes } };

    let env = ExecutorEnv::builder()
        .write_slice(core::slice::from_ref(&public_input))
        .write_slice(core::slice::from_ref(&state))
        .build()
        .unwrap();

    // Obtain the default prover.
    let prover = default_prover();

    let now = Instant::now();
    let prove_info = prover.prove_with_opts(env, ZK_COVENANT_INLINE_GUEST_ELF, &ProverOpts::succinct()).unwrap();
    println!("Proving took {} ms", now.elapsed().as_millis());

    // extract the receipt.
    let receipt = prove_info.receipt;
    let journal_digest = receipt.journal.digest();
    let receipt_inner = receipt.inner.succinct().unwrap();

    // Extract committed data from journal
    let journal_bytes = &receipt.journal.bytes;
    println!("Journal bytes: {}", faster_hex::hex_string(journal_bytes));

    let committed_public_input: zk_covenant_inline_core::PublicInput =
        *bytemuck::from_bytes(&journal_bytes[..size_of::<PublicInput>()]);
    assert_eq!(public_input, committed_public_input);
    let new_state_hash_from_journal: &[u8] = &journal_bytes[size_of::<PublicInput>()..size_of::<PublicInput>() + 32];
    let new_state_hash = new_state.hash();
    assert_eq!(new_state_hash_from_journal, bytemuck::bytes_of(&new_state_hash));
    // println!("New state hash: {}", faster_hex::hex_string(new_state_hash_from_journal));
    // println!("Old state hash: {}", faster_hex::hex_string(bytemuck::bytes_of(&prev_state_hash)));

    let expected_digest = {
        let mut pre_image = [0u8; 68];
        pre_image[..size_of::<PublicInput>()].copy_from_slice(bytemuck::bytes_of(&public_input));
        pre_image[size_of::<PublicInput>()..size_of::<PublicInput>() + 32].copy_from_slice(new_state_hash_from_journal);
        pre_image.digest()
    };
    assert_eq!(journal_digest, expected_digest);

    let script_precompile_inner = SuccinctReceipt {
        seal: receipt_inner.seal.clone(),
        control_id: receipt_inner.control_id,
        claim: receipt_inner.claim.digest(),
        hashfn: receipt_inner.hashfn.clone(),
        verifier_parameters: receipt_inner.verifier_parameters,
        control_inclusion_proof: MerkleProof::new(
            receipt_inner.control_inclusion_proof.index,
            receipt_inner.control_inclusion_proof.digests.clone(),
        ),
    };

    receipt.verify(ZK_COVENANT_INLINE_GUEST_ID).unwrap();

    // Build redeem scripts using 32-byte hashes
    let computed_len = build_redeem_script(public_input.prev_state_hash, 76).len() as i64;

    let input_redeem_script = build_redeem_script(public_input.prev_state_hash, computed_len);
    let input_spk = pay_to_script_hash_script(&input_redeem_script);
    let output_redeem_script = build_redeem_script(new_state_hash, computed_len);
    let output_spk = pay_to_script_hash_script(&output_redeem_script);
    assert_eq!(computed_len as usize, output_redeem_script.len());
    assert_eq!(computed_len as usize, input_redeem_script.len());

    // println!("Output redeem script: {}", faster_hex::hex_string(&output_redeem_script));
    // println!("output spk: {}", faster_hex::hex_string(&output_spk.to_bytes()));

    let payload = bytemuck::bytes_of(&public_input)[..size_of::<VersionedActionRaw>()].to_vec();
    let (mut tx, _, utxo_entry) = make_mock_transaction(0, input_spk, output_spk, payload);

    // --- Update the sig_script with the real proof and verify on-chain ---
    let final_sig_script = ScriptBuilder::new()
        .add_data(&borsh::to_vec(&script_precompile_inner).unwrap())
        .unwrap()
        .add_data(bytemuck::cast_slice(ZK_COVENANT_INLINE_GUEST_ID.as_slice()))
        .unwrap()
        .add_data(new_state_hash_from_journal)
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
    payload: Vec<u8>,
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
        payload,
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

fn build_redeem_script(old_state_hash: [u32; 8], redeem_script_len: i64) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // Prepare old and new states on the stack
    add_state_preparation(&mut builder, old_state_hash).unwrap();

    // Stack: [proof, program_id, old_state_hash, new_state_hash, new_state_hash]

    // Build the prefix for the new redeem script (OpData32 || new_state_hash)
    add_new_redeem_prefix(&mut builder).unwrap();

    // Stack: [proof, program_id, old_state_hash, new_state_hash, (OpData32 || new_state_hash)]

    // Extract the suffix from sig_script and concatenate to form the new redeem script
    add_suffix_extraction_and_cat(&mut builder, redeem_script_len).unwrap();

    // Stack: [proof, program_id, old_state_hash, new_state_hash, new_redeem_script]

    // Hash the new redeem script and build the expected SPK bytes
    add_hash_and_build_spk(&mut builder).unwrap();

    // Stack: [proof, program_id, old_state_hash, new_state_hash, constructed_spk]

    // Verify the constructed SPK matches the actual output SPK
    add_verify_output_spk(&mut builder).unwrap();

    // Stack: [proof, program_id, old_state_hash, new_state_hash]

    // Construct the preimage for the journal hash (versioned_action_raw || old_state_hash || new_state_hash)
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

/// Prepares the old and new state hashes on the stack by pushing the old state hash, swapping, and duplicating the new state hash.
///
/// Expects on stack: [proof, program_id, new_state_hash]
///
/// Leaves on stack: [proof, program_id, old_state_hash, new_state_hash, new_state_hash]
fn add_state_preparation(
    builder: &mut ScriptBuilder,
    old_state_hash: [u32; 8],
) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push the old state hash (prev_state_hash) onto the stack
    builder.add_data(bytemuck::bytes_of(&old_state_hash))?;
    // Swap the top two items: new_state_hash and old_state_hash
    builder.add_op(OpSwap)?;
    // Duplicate the new_state_hash (now on top)
    builder.add_op(OpDup)
}

/// Builds the prefix for the new redeem script by concatenating OpData32 with a duplicated new_state_hash.
///
/// Expects on stack: [..., old_state_hash, new_state_hash, new_state_hash]
///
/// Leaves on stack: [..., old_state_hash, new_state_hash, (OpData32 || new_state_hash)]
fn add_new_redeem_prefix(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Push OpData32 (for pushing 32-byte state hash in the new redeem script)
    builder.add_data(&[OpData32])?;
    // Swap to bring one new_state_hash to top
    builder.add_op(OpSwap)?;
    // Concatenate: OpData32 || new_state_hash
    builder.add_op(OpCat)
}

/// Extracts the redeem script suffix from the signature script and concatenates it with the prefix to form the new redeem script.
///
/// The suffix is extracted using a computed offset to skip the old state hash push in the current redeem script.
///
/// Expects on stack: [..., (OpData32 || new_state_hash)] (prefix on top)
///
/// Leaves on stack: [..., new_redeem_script]
fn add_suffix_extraction_and_cat(builder: &mut ScriptBuilder, redeem_script_len: i64) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Compute offset: sig_script_len + (-redeem_script_len + 33) to point after old_state_hash push
    builder.add_op(OpTxInputIndex)?;
    builder.add_op(OpTxInputIndex)?;
    builder.add_op(OpTxInputScriptSigLen)?;
    builder.add_i64(-redeem_script_len + 33)?; // -redeem script + OpData32 + 32 bytes
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

/// Constructs the preimage for the journal hash by concatenating versioned_action_raw || old_state_hash || new_state_hash.
///
/// Expects on stack: [..., old_state_hash, new_state_hash]
///
/// Leaves on stack: [..., preimage] where preimage = versioned_action_raw || old_state_hash || new_state_hash
fn add_construct_journal_preimage(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Concatenate old_state_hash || new_state_hash
    builder.add_op(OpCat)?;

    // Push start (0) and end (4) for payload substr
    builder.add_i64(0)?;
    builder.add_i64(size_of::<VersionedActionRaw>() as i64)?;

    // Extract versioned_action_raw = tx.payload[0..4]
    builder.add_op(OpTxPayloadSubstr)?;

    // Concatenate versioned_action_raw || (old_state_hash || new_state_hash)
    builder.add_op(OpSwap)?;
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