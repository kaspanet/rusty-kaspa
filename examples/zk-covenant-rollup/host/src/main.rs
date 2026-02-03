mod covenant;
mod mock_chain;
mod mock_tx;
mod redeem;
mod tx;

use std::time::Instant;

use kaspa_hashes::Hash;
use kaspa_txscript::{
    pay_to_script_hash_script,
    script_builder::ScriptBuilder,
    zk_precompiles::{risc0::merkle::MerkleProof, risc0::rcpt::SuccinctReceipt, tags::ZkTag},
};
use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv, Prover, ProverOpts};
use zk_covenant_rollup_core::{state::State, PublicInput};
use zk_covenant_rollup_methods::{ZK_COVENANT_ROLLUP_GUEST_ELF, ZK_COVENANT_ROLLUP_GUEST_ID};

use mock_chain::{build_mock_chain, calc_accepted_id_merkle_root, from_bytes};
use zk_covenant_common::seal_to_compressed_proof;

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env()).init();

    // Initialize state and seq_commitment
    let initial_state = State::default();
    let prev_state_hash = initial_state.hash();
    let prev_seq_commit_hash = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
    let prev_seq_commitment = from_bytes(prev_seq_commit_hash.as_bytes());

    println!("=== ZK Rollup Covenant Demo ===");
    println!("Initial state hash: {}", faster_hex::hex_string(bytemuck::bytes_of(&prev_state_hash)));
    println!("Initial seq_commitment: {}", prev_seq_commit_hash);

    // Build mock chain with 3 blocks
    let chain = build_mock_chain(3, prev_seq_commit_hash, initial_state);
    let new_state_hash = chain.final_state.hash();
    let new_seq_commitment = from_bytes(chain.final_seq_commit.as_bytes());

    println!("\nFinal state hash: {}", faster_hex::hex_string(bytemuck::bytes_of(&new_state_hash)));
    println!("Final seq_commitment: {}", chain.final_seq_commit);

    let public_input = PublicInput { prev_state_hash, prev_seq_commitment };
    let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);

    // Build executor env closure
    let build_env = || {
        let mut binding = ExecutorEnv::builder();
        let builder = binding
            .write_slice(core::slice::from_ref(&public_input))
            .write_slice(State::default().as_word_slice())
            .write_slice(&(chain.block_txs.len() as u32).to_le_bytes());

        for txs in &chain.block_txs {
            builder.write_slice(&(txs.len() as u32).to_le_bytes());
            for tx in txs {
                tx.write_to_env(builder);
            }
        }
        builder.build().unwrap()
    };

    let prover = default_prover();

    // === STARK (Succinct) Proof ===
    println!("\n=== Proving with RISC0 (Succinct/STARK) ===");
    let now = Instant::now();
    let succinct_info = prover.prove_with_opts(build_env(), ZK_COVENANT_ROLLUP_GUEST_ELF, &ProverOpts::succinct()).unwrap();
    println!("Succinct proving took {} ms", now.elapsed().as_millis());

    let succinct_receipt = succinct_info.receipt;
    verify_journal(&succinct_receipt.journal.bytes, &public_input, &new_state_hash, &new_seq_commitment);
    succinct_receipt.verify(ZK_COVENANT_ROLLUP_GUEST_ID).unwrap();
    println!("Succinct proof verified!");

    // On-chain verification for STARK
    let block_prove_to = *chain.block_hashes.last().unwrap();
    if let Ok(receipt_inner) = succinct_receipt.inner.succinct() {
        println!("\n=== On-chain STARK verification ===");
        let script_receipt = SuccinctReceipt {
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
        let proof_bytes = borsh::to_vec(&script_receipt).unwrap();
        verify_onchain_with_proof(
            &proof_bytes,
            &public_input,
            &new_state_hash,
            &new_seq_commitment,
            block_prove_to,
            &chain,
            &program_id,
            &ZkTag::R0Succinct,
        );
        println!("STARK on-chain verification passed!");
    } else {
        println!("Skipping STARK on-chain verification (dev mode)");
    }

    // === Groth16 Proof ===
    println!("\n=== Proving with RISC0 (Groth16) ===");
    let now = Instant::now();
    let groth16_info = prover.prove_with_opts(build_env(), ZK_COVENANT_ROLLUP_GUEST_ELF, &ProverOpts::groth16()).unwrap();
    println!("Groth16 proving took {} ms", now.elapsed().as_millis());

    let groth16_receipt = groth16_info.receipt;
    verify_journal(&groth16_receipt.journal.bytes, &public_input, &new_state_hash, &new_seq_commitment);
    groth16_receipt.verify(ZK_COVENANT_ROLLUP_GUEST_ID).unwrap();
    println!("Groth16 proof verified!");

    // On-chain verification for Groth16
    if let Ok(groth16_inner) = groth16_receipt.inner.groth16() {
        println!("\n=== On-chain Groth16 verification ===");
        let compressed_proof = seal_to_compressed_proof(&groth16_inner.seal);

        verify_onchain_with_proof(
            &compressed_proof,
            &public_input,
            &new_state_hash,
            &new_seq_commitment,
            block_prove_to,
            &chain,
            &program_id,
            &ZkTag::Groth16,
        );
        println!("Groth16 on-chain verification passed!");
    } else {
        println!("Skipping Groth16 on-chain verification (dev mode)");
    }

    println!("\n=== All verifications passed! ===");
}

fn verify_journal(journal: &[u8], public_input: &PublicInput, new_state_hash: &[u32; 8], new_seq_commitment: &[u32; 8]) {
    let pi_size = size_of::<PublicInput>();
    let committed_pi: PublicInput = *bytemuck::from_bytes(&journal[..pi_size]);
    assert_eq!(*public_input, committed_pi, "PublicInput mismatch");
    assert_eq!(&journal[pi_size..pi_size + 32], bytemuck::bytes_of(new_state_hash));
    assert_eq!(&journal[pi_size + 32..pi_size + 64], bytemuck::bytes_of(new_seq_commitment));
}

fn verify_onchain_with_proof(
    proof_bytes: &[u8],
    public_input: &PublicInput,
    new_state_hash: &[u32; 8],
    new_seq_commitment: &[u32; 8],
    block_prove_to: Hash,
    chain: &mock_chain::MockChain,
    program_id: &[u8; 32],
    zk_tag: &ZkTag,
) {
    let mut computed_len = 75;
    loop {
        let script = redeem::build_redeem_script(
            public_input.prev_state_hash,
            public_input.prev_seq_commitment,
            computed_len,
            program_id,
            zk_tag,
        );
        let new_len = script.len() as i64;
        if new_len == computed_len {
            break;
        }
        computed_len = new_len;
    }

    let input_redeem =
        redeem::build_redeem_script(public_input.prev_state_hash, public_input.prev_seq_commitment, computed_len, program_id, zk_tag);
    let output_redeem = redeem::build_redeem_script(*new_state_hash, *new_seq_commitment, computed_len, program_id, zk_tag);

    // Build transaction
    let (mut tx, utxo) =
        tx::make_mock_transaction(0, pay_to_script_hash_script(&input_redeem), pay_to_script_hash_script(&output_redeem));

    // Build sig_script: [proof, block_prove_to, new_state_hash, redeem]
    tx.inputs[0].signature_script = ScriptBuilder::new()
        .add_data(proof_bytes)
        .unwrap()
        .add_data(block_prove_to.as_bytes().as_slice())
        .unwrap()
        .add_data(bytemuck::bytes_of(new_state_hash))
        .unwrap()
        .add_data(&input_redeem)
        .unwrap()
        .drain();

    tx::verify_tx(&tx, &utxo, &chain.accessor);
}
