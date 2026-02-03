use crate::covenant::InlineCovenant;
use kaspa_consensus_core::tx::CovenantBinding;
use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    hashing::sighash::SigHashReusedValuesUnsync,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_hashes::Hash;
use kaspa_txscript::{
    caches::Cache,
    covenants::CovenantsContext,
    opcodes::codes::{OpTrue, OpVerify, OpZkPrecompile},
    pay_to_script_hash_script,
    script_builder::ScriptBuilder,
    zk_precompiles::{risc0::merkle::MerkleProof, risc0::rcpt::SuccinctReceipt, tags::ZkTag},
    EngineCtx, EngineFlags, TxScriptEngine,
};
use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv, Prover, ProverOpts};
use std::time::Instant;
use zk_covenant_common::{seal_to_compressed_proof, CovenantBase, Risc0Groth16Verify};
use zk_covenant_inline_core::{Action, PublicInput, State, VersionedActionRaw};
use zk_covenant_inline_methods::{ZK_COVENANT_INLINE_GUEST_ELF, ZK_COVENANT_INLINE_GUEST_ID};

mod covenant;

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env()).init();

    let state = State::default();
    let prev_state_hash = state.hash();
    let action = Action::Fib(5);
    let new_state = {
        let mut state = State::default();
        state.add_new_result(action, 5);
        state
    };

    let (action_disc, action_value) = action.split();
    let action_bytes = [action_disc, action_value];
    let public_input =
        PublicInput { prev_state_hash, versioned_action_raw: VersionedActionRaw { action_version: 0, action_raw: action_bytes } };

    let env = || {
        ExecutorEnv::builder()
            .write_slice(core::slice::from_ref(&public_input))
            .write_slice(core::slice::from_ref(&state))
            .build()
            .unwrap()
    };

    let prover = default_prover();

    // --- Succinct (STARK) proof ---
    let now = Instant::now();
    let succinct_prove_info = prover.prove_with_opts(env(), ZK_COVENANT_INLINE_GUEST_ELF, &ProverOpts::succinct()).unwrap();
    println!("Succinct proving took {} ms", now.elapsed().as_millis());

    let succinct_receipt = succinct_prove_info.receipt;
    succinct_receipt.verify(ZK_COVENANT_INLINE_GUEST_ID).unwrap();

    // Extract committed data from journal (shared between both proofs)
    let journal_bytes = &succinct_receipt.journal.bytes;
    let committed_public_input: PublicInput = *bytemuck::from_bytes(&journal_bytes[..size_of::<PublicInput>()]);
    assert_eq!(public_input, committed_public_input);

    let new_state_hash_from_journal: &[u8] = &journal_bytes[size_of::<PublicInput>()..size_of::<PublicInput>() + 32];
    let new_state_hash = new_state.hash();
    assert_eq!(new_state_hash_from_journal, bytemuck::bytes_of(&new_state_hash));

    let journal_digest = succinct_receipt.journal.digest();
    let expected_digest = {
        let mut pre_image = [0u8; 68];
        pre_image[..size_of::<PublicInput>()].copy_from_slice(bytemuck::bytes_of(&public_input));
        pre_image[size_of::<PublicInput>()..size_of::<PublicInput>() + 32].copy_from_slice(new_state_hash_from_journal);
        pre_image.digest()
    };
    assert_eq!(journal_digest, expected_digest);

    succinct_receipt.verify(ZK_COVENANT_INLINE_GUEST_ID).unwrap();

    // --- Verify STARK (succinct) on-chain ---
    let receipt_inner = succinct_receipt.inner.succinct().unwrap();
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

    let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_INLINE_GUEST_ID);
    let computed_len = build_redeem_script(public_input.prev_state_hash, 76, &program_id, ZkTag::R0Succinct).len() as i64;

    let input_redeem_script = build_redeem_script(public_input.prev_state_hash, computed_len, &program_id, ZkTag::R0Succinct);
    let output_redeem_script = build_redeem_script(new_state_hash, computed_len, &program_id, ZkTag::R0Succinct);

    let payload = bytemuck::bytes_of(&public_input)[..size_of::<VersionedActionRaw>()].to_vec();
    let (mut tx, _, utxo_entry) = make_mock_transaction(
        0,
        pay_to_script_hash_script(&input_redeem_script),
        pay_to_script_hash_script(&output_redeem_script),
        payload.clone(),
    );

    let proof_bytes = borsh::to_vec(&script_precompile_inner).unwrap();
    let final_sig_script = build_final_signature_script(&proof_bytes, new_state_hash_from_journal, &input_redeem_script);
    tx.inputs[0].signature_script = final_sig_script;

    verify_tx(&tx, &utxo_entry);
    println!("STARK (succinct) proof verified successfully on-chain!");

    // --- Groth16 proof ---
    let now = Instant::now();
    let groth16_prove_info = prover.prove_with_opts(env(), ZK_COVENANT_INLINE_GUEST_ELF, &ProverOpts::groth16()).unwrap();
    println!("Groth16 proving took {} ms", now.elapsed().as_millis());

    let groth16_receipt = groth16_prove_info.receipt;
    groth16_receipt.verify(ZK_COVENANT_INLINE_GUEST_ID).unwrap();

    let groth16_inner = groth16_receipt.inner.groth16().unwrap();
    let seal = &groth16_inner.seal;
    let compressed_proof = seal_to_compressed_proof(seal);

    let computed_len_g16 = build_redeem_script(public_input.prev_state_hash, 1011, &program_id, ZkTag::Groth16).len() as i64;
    let input_redeem_g16 = build_redeem_script(public_input.prev_state_hash, computed_len_g16, &program_id, ZkTag::Groth16);
    let output_redeem_g16 = build_redeem_script(new_state_hash, computed_len_g16, &program_id, ZkTag::Groth16);

    let (mut tx_g16, _, utxo_entry_g16) =
        make_mock_transaction(0, pay_to_script_hash_script(&input_redeem_g16), pay_to_script_hash_script(&output_redeem_g16), payload);

    let final_sig_script_g16 = build_final_signature_script(&compressed_proof, new_state_hash_from_journal, &input_redeem_g16);
    tx_g16.inputs[0].signature_script = final_sig_script_g16;

    verify_tx(&tx_g16, &utxo_entry_g16);
    println!("Groth16 proof verified successfully on-chain!");
}

// Unified final signature script builder (used by both STARK and Groth16)
fn build_final_signature_script(proof: &[u8], new_state_hash: &[u8], redeem_script: &[u8]) -> Vec<u8> {
    ScriptBuilder::new().add_data(proof).unwrap().add_data(new_state_hash).unwrap().add_data(redeem_script).unwrap().drain()
}

fn make_mock_transaction(
    lock_time: u64,
    input_spk: ScriptPublicKey,
    output_spk: ScriptPublicKey,
    payload: Vec<u8>,
) -> (Transaction, TransactionInput, UtxoEntry) {
    let dummy_prev_out = TransactionOutpoint::new(kaspa_hashes::Hash::from_u64_word(1), 1);
    let dummy_tx_input = TransactionInput::new(dummy_prev_out, vec![], 10, u8::MAX);

    let cov_id = Hash::from_bytes([0xFF; _]);
    let dummy_tx_out = TransactionOutput::with_covenant(
        SOMPI_PER_KASPA,
        output_spk,
        Some(CovenantBinding { authorizing_input: 0, covenant_id: cov_id }),
    );

    let tx = Transaction::new(
        TX_VERSION + 1,
        vec![dummy_tx_input.clone()],
        vec![dummy_tx_out.clone()],
        lock_time,
        SUBNETWORK_ID_NATIVE,
        0,
        payload,
    );
    let utxo_entry = UtxoEntry::new(0, input_spk, 0, false, Some(cov_id));
    (tx, dummy_tx_input, utxo_entry)
}

fn verify_tx(tx: &Transaction, utxo_entry: &UtxoEntry) {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, vec![utxo_entry.clone()]);
    let covenant_ctx = CovenantsContext::from_tx(&populated).unwrap();
    let ctx = EngineCtx::new(&sig_cache).with_reused(&reused_values).with_covenants_ctx(&covenant_ctx);
    let mut vm = TxScriptEngine::from_transaction_input(&populated, &tx.inputs[0], 0, utxo_entry, ctx, flags);
    vm.execute().unwrap();
}

fn build_redeem_script(old_state_hash: [u32; 8], redeem_script_len: i64, program_id: &[u8; 32], zk_tag: ZkTag) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // Expects on sig_script stack: [proof, new_state_hash]
    builder.push_old_state_and_dup_new(old_state_hash).unwrap();
    // Stack: [proof, old_state_hash, new_state_hash, new_state_hash]

    builder.build_next_redeem_prefix().unwrap();
    // Stack: [proof, old_state_hash, new_state_hash, (OpData32 || new_state_hash)]

    builder.extract_redeem_suffix_and_concat(redeem_script_len).unwrap();
    // Stack: [proof, old_state_hash, new_state_hash, new_redeem_script]

    builder.hash_redeem_to_spk().unwrap();
    // Stack: [proof, old_state_hash, new_state_hash, constructed_spk]

    builder.verify_output_spk().unwrap();
    // Stack: [proof, old_state_hash, new_state_hash]

    builder.build_journal_preimage().unwrap();
    // Stack: [proof,  preimage]

    builder.hash_journal().unwrap();
    // Stack: [proof, journal_hash]

    // Hardcode program_id in redeem script (no longer in sig_script)
    builder.add_data(program_id).unwrap();
    // Stack: [proof, journal_hash, program_id]

    match zk_tag {
        ZkTag::R0Succinct => {
            // Push succinct ZK tag and verify
            builder.add_data(&[ZkTag::R0Succinct as u8]).unwrap();
            // Stack: [proof, journal_hash, program_id, ZkTag::R0Succinct]

            builder.add_op(OpZkPrecompile).unwrap();
            // Stack: [true]
            builder.add_op(OpVerify).unwrap();
            // Stack: []
        }
        ZkTag::Groth16 => {
            builder.verify_risc0_groth16().unwrap();
            // Stack: []
        }
    }
    builder.verify_input_index_zero().unwrap();
    // Stack: []

    builder.verify_covenant_single_output().unwrap();

    builder.add_op(OpTrue).unwrap();
    // Stack: [true]
    builder.drain()
}
