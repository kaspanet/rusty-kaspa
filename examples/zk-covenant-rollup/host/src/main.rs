mod covenant;

use std::{collections::HashMap, iter, time::Instant};

use crate::covenant::RollupCovenant;
use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    hashing::sighash::SigHashReusedValuesUnsync,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
        UtxoEntry,
    },
};
use kaspa_hashes::{Hash, SeqCommitmentMerkleBranchHash};
use kaspa_txscript::{
    caches::Cache,
    covenants::CovenantsContext,
    engine_context::EngineContext,
    opcodes::codes::{OpTrue, OpVerify, OpZkPrecompile},
    pay_to_script_hash_script,
    script_builder::ScriptBuilder,
    seq_commit_accessor::SeqCommitAccessor,
    zk_precompiles::{risc0::merkle::MerkleProof, risc0::rcpt::SuccinctReceipt, tags::ZkTag},
    EngineFlags, TxScriptEngine,
};
use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv, Prover, ProverOpts};
use zk_covenant_rollup_core::{state::State, PublicInput};
use zk_covenant_rollup_methods::{ZK_COVENANT_ROLLUP_GUEST_ELF, ZK_COVENANT_ROLLUP_GUEST_ID};

struct MockSeqCommitAccessor(HashMap<Hash, Hash>);

impl SeqCommitAccessor for MockSeqCommitAccessor {
    fn is_chain_ancestor_from_pov(&self, block_hash: Hash) -> Option<bool> {
        self.0.contains_key(&block_hash).then_some(true)
    }

    fn seq_commitment_within_depth(&self, block_hash: Hash) -> Option<Hash> {
        self.0.get(&block_hash).copied()
    }
}

fn main() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env()).init();

    // --- State doesn't change (no action txs) ---
    let state = State::default();
    let prev_state_hash = state.hash();
    let new_state_hash = prev_state_hash; // no actions → state unchanged

    // --- Build chain of 3 empty blocks, compute seq_commitment chain ---
    let prev_seq_commitment_hash = calc_accepted_id_merkle_root(Hash::default(), iter::empty());
    let prev_seq_commitment = from_bytes(prev_seq_commitment_hash.as_bytes());

    let chain_len = 3u32;
    let block_hashes: Vec<Hash> = (1..=chain_len).map(|i| Hash::from_u64_word(i as u64)).collect();

    let mut seq_commit = prev_seq_commitment_hash;
    let mut accessor_map = HashMap::new();
    for &block_hash in &block_hashes {
        seq_commit = calc_accepted_id_merkle_root(seq_commit, iter::empty());
        accessor_map.insert(block_hash, seq_commit);
    }

    let block_prove_to = *block_hashes.last().unwrap();
    let new_seq_commitment_hash = seq_commit;
    let new_seq_commitment = from_bytes(new_seq_commitment_hash.as_bytes());

    let public_input = PublicInput { prev_state_hash, prev_seq_commitment };

    // On-chain preimage order: prev_state_hash || prev_seq_commitment || new_state_hash || new_seq_commitment
    let mut journal_preimage = [0u8; 128];
    journal_preimage[0..32].copy_from_slice(bytemuck::bytes_of(&prev_state_hash));
    journal_preimage[32..64].copy_from_slice(bytemuck::bytes_of(&prev_seq_commitment));
    journal_preimage[64..96].copy_from_slice(bytemuck::bytes_of(&new_state_hash));
    journal_preimage[96..128].copy_from_slice(bytemuck::bytes_of(&new_seq_commitment));

    // --- Prove with RISC0 ---
    let mut binding = ExecutorEnv::builder();
    let builder = binding
        .write_slice(core::slice::from_ref(&public_input))
        .write_slice(core::slice::from_ref(&state))
        .write_slice(&chain_len.to_le_bytes());
    for _ in 0..chain_len {
        let tx_count = 0u32;
        builder.write_slice(&tx_count.to_le_bytes());
    }

    let env = builder.build().unwrap();
    let prover = default_prover();

    let now = Instant::now();
    let prove_info = prover.prove_with_opts(env, ZK_COVENANT_ROLLUP_GUEST_ELF, &ProverOpts::succinct()).unwrap();
    println!("Proving took {} ms", now.elapsed().as_millis());

    let receipt = prove_info.receipt;
    let journal_bytes = &receipt.journal.bytes;

    // --- Verify journal contents ---
    let pi_size = size_of::<PublicInput>();
    let committed_pi: PublicInput = *bytemuck::from_bytes(&journal_bytes[..pi_size]);
    assert_eq!(public_input, committed_pi);

    let new_state_hash_bytes = &journal_bytes[pi_size..pi_size + 32];
    assert_eq!(new_state_hash_bytes, bytemuck::bytes_of(&new_state_hash));

    let new_seq_commitment_bytes = &journal_bytes[pi_size + 32..pi_size + 64];
    assert_eq!(new_seq_commitment_bytes, bytemuck::bytes_of(&new_seq_commitment));

    // === DEBUG: compare journal digest from receipt vs manual ===
    // let journal_digest = receipt.journal.digest();
    // println!("journal digest (risc0):  {:?}", journal_digest);

    assert_eq!(journal_preimage.as_slice(), journal_bytes);
    assert_eq!(journal_preimage.as_slice().digest(), receipt.journal.digest());

    let receipt_inner = receipt.inner.succinct().unwrap();
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

    receipt.verify(ZK_COVENANT_ROLLUP_GUEST_ID).unwrap();

    // --- Build redeem scripts ---
    let program_id: &[u8] = bytemuck::cast_slice(ZK_COVENANT_ROLLUP_GUEST_ID.as_slice());

    // First pass to compute length
    let computed_len = build_redeem_script(prev_state_hash, prev_seq_commitment, 131, program_id).len() as i64;

    let input_redeem_script = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, program_id);
    let input_spk = pay_to_script_hash_script(&input_redeem_script);
    let output_redeem_script = build_redeem_script(new_state_hash, new_seq_commitment, computed_len, program_id);
    let output_spk = pay_to_script_hash_script(&output_redeem_script);
    assert_eq!(computed_len as usize, output_redeem_script.len());
    assert_eq!(computed_len as usize, input_redeem_script.len());

    // --- Build mock transaction ---
    let (mut tx, _, utxo_entry) = make_mock_transaction(0, input_spk.clone(), output_spk.clone());

    // --- Build sig_script: [proof, block_prove_to, new_app_state_hash, redeem] ---
    let final_sig_script = ScriptBuilder::new()
        .add_data(&borsh::to_vec(&script_precompile_inner).unwrap())
        .unwrap()
        .add_data(block_prove_to.as_bytes().as_slice())
        .unwrap()
        .add_data(bytemuck::bytes_of(&new_state_hash))
        .unwrap()
        .add_data(&input_redeem_script)
        .unwrap()
        .drain();

    tx.inputs[0].signature_script = final_sig_script;

    let accessor = MockSeqCommitAccessor(accessor_map);

    // println!("\n=== Testing full redeem script ===");
    // println!("  input_redeem len={}", input_redeem_script.len());
    // println!("  output_redeem len={}", output_redeem_script.len());
    // println!("  sig_script len={}", tx.inputs[0].signature_script.len());
    // println!("  input_redeem first 66 bytes: {}", faster_hex::hex_string(&input_redeem_script[..66]));
    // println!("  output_redeem first 66 bytes: {}", faster_hex::hex_string(&output_redeem_script[..66]));
    // println!("  suffix same? {}", input_redeem_script[66..] == output_redeem_script[66..]);
    verify_tx(&tx, &utxo_entry, &accessor);
    println!("ZK proof verified successfully on-chain!");
}

fn build_redeem_script(
    prev_state_hash: [u32; 8],
    prev_seq_commitment: [u32; 8],
    redeem_script_len: i64,
    program_id: &[u8],
) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // Redeem script starts with 66-byte prefix encoding the prev values:
    // OpData32 || prev_seq_commitment || OpData32 || prev_app_state_hash
    // When executed, these push prev_seq_commitment and prev_state_hash onto the stack.
    builder.add_data(bytemuck::bytes_of(&prev_seq_commitment)).unwrap();
    builder.add_data(bytemuck::bytes_of(&prev_state_hash)).unwrap();
    // Stack: [proof, block_prove_to, new_app_state_hash, prev_seq_commitment, prev_state_hash]

    // Stash prev values to alt stack for later use in journal
    builder.stash_prev_values().unwrap();
    // Stack: [proof, block_prove_to, new_app_state_hash], alt:[prev_state_hash, prev_seq_commitment]

    // 1. Get new_seq_commitment from block_prove_to
    builder.obtain_new_seq_commitment().unwrap();
    // Stack: [proof, new_app_state_hash, new_seq_commitment], alt:[prev_state_hash, prev_seq_commitment]

    // 2. Build new redeem prefix (66 bytes) and stash new values on alt stack
    builder.build_next_redeem_prefix_rollup().unwrap();
    // Stack: [proof, 66-byte-prefix], alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]

    // 3. Extract suffix and concat
    builder.extract_redeem_suffix_and_concat(redeem_script_len).unwrap();
    // Stack: [proof, new_redeem_script], alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]

    // 4. Hash redeem → SPK, verify output
    builder.hash_redeem_to_spk().unwrap();
    builder.verify_output_spk().unwrap();
    // Stack: [proof], alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]

    // 5. Build journal preimage and hash (all values from alt stack)
    builder.build_and_hash_journal().unwrap();
    // Stack: [proof, journal_hash]

    // // skip zk check
    // builder.add_op(Op2Drop).unwrap();

    // 6. ZK verify
    builder.add_data(program_id).unwrap();
    builder.add_data(&[ZkTag::R0Succinct as u8]).unwrap();
    builder.add_op(OpZkPrecompile).unwrap();
    builder.add_op(OpVerify).unwrap();
    // Stack: []

    // 7. Guards
    builder.verify_input_index_zero().unwrap();
    builder.verify_covenant_single_output().unwrap();
    builder.add_op(OpTrue).unwrap();

    builder.drain()
}

fn make_mock_transaction(
    lock_time: u64,
    input_spk: ScriptPublicKey,
    output_spk: ScriptPublicKey,
) -> (Transaction, TransactionInput, UtxoEntry) {
    let dummy_prev_out = TransactionOutpoint::new(Hash::from_u64_word(1), 1);
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
        vec![],
    );
    let utxo_entry = UtxoEntry::new(0, input_spk, 0, false, Some(cov_id));
    (tx, dummy_tx_input, utxo_entry)
}

fn verify_tx(tx: &Transaction, utxo_entry: &UtxoEntry, accessor: &dyn SeqCommitAccessor) {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, vec![utxo_entry.clone()]);
    let cov_ctx = CovenantsContext::from_tx(&populated).unwrap();
    let exec_ctx =
        EngineContext::new(&sig_cache).with_reused(&reused_values).with_seq_commit_accessor(accessor).with_covenants_ctx(&cov_ctx);
    let mut vm = TxScriptEngine::from_transaction_input(&populated, &tx.inputs[0], 0, utxo_entry, exec_ctx, flags);
    vm.execute().unwrap();
}

fn calc_accepted_id_merkle_root(prev_accepted_id_merkle_root: Hash, accepted_tx_digests: impl ExactSizeIterator<Item = Hash>) -> Hash {
    kaspa_merkle::merkle_hash_with_hasher(
        prev_accepted_id_merkle_root,
        kaspa_merkle::calc_merkle_root_with_hasher::<SeqCommitmentMerkleBranchHash, true>(accepted_tx_digests),
        SeqCommitmentMerkleBranchHash::new(),
    )
}

fn from_bytes(arr: [u8; 32]) -> [u32; 8] {
    let mut out = [0; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(&arr);
    out
}
