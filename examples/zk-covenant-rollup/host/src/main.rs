mod covenant;
mod mock_chain;
mod mock_tx;
mod redeem;
mod tx;

use std::time::Instant;

use kaspa_hashes::Hash;
use kaspa_txscript::{
    pay_to_script_hash_script, script_builder::ScriptBuilder,
    zk_precompiles::{risc0::merkle::MerkleProof, risc0::rcpt::SuccinctReceipt},
};
use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv, Prover, ProverOpts};
use zk_covenant_rollup_core::{state::State, PublicInput};
use zk_covenant_rollup_methods::{ZK_COVENANT_ROLLUP_GUEST_ELF, ZK_COVENANT_ROLLUP_GUEST_ID};

use mock_chain::{build_mock_chain, calc_accepted_id_merkle_root, from_bytes};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

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

    // Prove with RISC0
    let public_input = PublicInput { prev_state_hash, prev_seq_commitment };
    let receipt = prove_rollup(&public_input, &chain);

    // Verify journal contents
    verify_journal(&receipt.journal.bytes, &public_input, &new_state_hash, &new_seq_commitment);
    receipt.verify(ZK_COVENANT_ROLLUP_GUEST_ID).unwrap();
    println!("ZK proof verified!");

    // On-chain verification
    let block_prove_to = *chain.block_hashes.last().unwrap();
    verify_onchain(&receipt, &public_input, &new_state_hash, block_prove_to, &chain);
}

fn prove_rollup(
    public_input: &PublicInput,
    chain: &mock_chain::MockChain,
) -> risc0_zkvm::Receipt {
    println!("\n=== Proving with RISC0 ===");
    let mut binding = ExecutorEnv::builder();
    let builder = binding
        .write_slice(core::slice::from_ref(public_input))
        .write_slice(State::default().as_word_slice())
        .write_slice(&(chain.block_txs.len() as u32).to_le_bytes());

    for txs in &chain.block_txs {
        builder.write_slice(&(txs.len() as u32).to_le_bytes());
        for tx in txs {
            tx.write_to_env(builder);
        }
    }

    let env = builder.build().unwrap();
    let now = Instant::now();
    let prove_info = default_prover()
        .prove_with_opts(env, ZK_COVENANT_ROLLUP_GUEST_ELF, &ProverOpts::succinct())
        .unwrap();
    println!("Proving took {} ms", now.elapsed().as_millis());

    prove_info.receipt
}

fn verify_journal(
    journal: &[u8],
    public_input: &PublicInput,
    new_state_hash: &[u32; 8],
    new_seq_commitment: &[u32; 8],
) {
    let pi_size = size_of::<PublicInput>();
    let committed_pi: PublicInput = *bytemuck::from_bytes(&journal[..pi_size]);
    assert_eq!(*public_input, committed_pi, "PublicInput mismatch");
    assert_eq!(&journal[pi_size..pi_size + 32], bytemuck::bytes_of(new_state_hash));
    assert_eq!(&journal[pi_size + 32..pi_size + 64], bytemuck::bytes_of(new_seq_commitment));
}

fn verify_onchain(
    receipt: &risc0_zkvm::Receipt,
    public_input: &PublicInput,
    new_state_hash: &[u32; 8],
    block_prove_to: Hash,
    chain: &mock_chain::MockChain,
) {
    let Ok(receipt_inner) = receipt.inner.succinct() else {
        println!("Skipping on-chain verification (no succinct receipt in dev mode)");
        return;
    };

    println!("\n=== On-chain verification ===");
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

    let program_id: &[u8] = bytemuck::cast_slice(ZK_COVENANT_ROLLUP_GUEST_ID.as_slice());
    let new_seq_commitment = from_bytes(chain.final_seq_commit.as_bytes());

    // Build redeem scripts
    let computed_len = redeem::build_redeem_script(
        public_input.prev_state_hash,
        public_input.prev_seq_commitment,
        131,
        program_id,
    ).len() as i64;

    let input_redeem = redeem::build_redeem_script(
        public_input.prev_state_hash,
        public_input.prev_seq_commitment,
        computed_len,
        program_id,
    );
    let output_redeem = redeem::build_redeem_script(
        *new_state_hash,
        new_seq_commitment,
        computed_len,
        program_id,
    );

    // Build and verify transaction
    let (mut tx, utxo) = tx::make_mock_transaction(
        0,
        pay_to_script_hash_script(&input_redeem),
        pay_to_script_hash_script(&output_redeem),
    );

    tx.inputs[0].signature_script = ScriptBuilder::new()
        .add_data(&borsh::to_vec(&script_receipt).unwrap()).unwrap()
        .add_data(block_prove_to.as_bytes().as_slice()).unwrap()
        .add_data(bytemuck::bytes_of(new_state_hash)).unwrap()
        .add_data(&input_redeem).unwrap()
        .drain();

    tx::verify_tx(&tx, &utxo, &chain.accessor);
    println!("On-chain script verification passed!");
}
