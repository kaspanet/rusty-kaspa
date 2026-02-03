mod covenant;

use std::{collections::HashMap, time::Instant};

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
use zk_covenant_rollup_core::{
    action::{Action, VersionedActionRaw},
    is_action_tx_id, payload_digest,
    seq_commit::seq_commitment_leaf,
    state::State,
    tx_id_v1, PublicInput, ACTION_TX_ID_PREFIX,
};
use zk_covenant_rollup_methods::{ZK_COVENANT_ROLLUP_GUEST_ELF, ZK_COVENANT_ROLLUP_GUEST_ID};

/// Represents a mock transaction to be included in a block
#[derive(Clone, Debug)]
enum MockTx {
    /// Version 0 tx: just a raw tx_id (no payload processing)
    V0 { tx_id: [u32; 8] },
    /// Version 1+ tx: has payload and rest_digest
    V1 { version: u16, payload: VersionedActionRaw, rest_digest: [u32; 8] },
}

impl MockTx {
    fn version(&self) -> u16 {
        match self {
            MockTx::V0 { .. } => 0,
            MockTx::V1 { version, .. } => *version,
        }
    }

    fn tx_id(&self) -> [u32; 8] {
        match self {
            MockTx::V0 { tx_id } => *tx_id,
            MockTx::V1 { payload, rest_digest, .. } => {
                let payload_words = payload.as_words();
                let payload_digest = payload_digest(payload_words);
                tx_id_v1(&payload_digest, rest_digest)
            }
        }
    }

    fn is_valid_action(&self) -> bool {
        match self {
            MockTx::V0 { .. } => false,
            MockTx::V1 { payload, .. } => {
                let tx_id = self.tx_id();
                if !is_action_tx_id(&tx_id) {
                    return false;
                }
                Action::try_from(payload.action_raw).is_ok()
            }
        }
    }

    /// Write to executor env in the format expected by guest
    fn write_to_env(&self, builder: &mut risc0_zkvm::ExecutorEnvBuilder<'_>) {
        let version = self.version() as u32;
        builder.write_slice(&version.to_le_bytes());

        match self {
            MockTx::V0 { tx_id } => {
                let bytes: &[u8] = bytemuck::cast_slice(tx_id);
                builder.write_slice(bytes);
            }
            MockTx::V1 { payload, rest_digest, .. } => {
                let payload_bytes: &[u8] = bytemuck::cast_slice(payload.as_words());
                builder.write_slice(payload_bytes);
                let rest_bytes: &[u8] = bytemuck::cast_slice(rest_digest);
                builder.write_slice(rest_bytes);
            }
        }
    }
}

/// Find a nonce that makes the tx_id start with ACTION_TX_ID_PREFIX (single byte)
fn find_action_tx_nonce(action: Action, action_version: u16, rest_digest: [u32; 8]) -> VersionedActionRaw {
    let (discriminator, value) = action.split();
    for nonce in 0u32.. {
        let payload = VersionedActionRaw { action_version, action_raw: [discriminator, value], nonce };
        let payload_words = payload.as_words();
        let pd = payload_digest(payload_words);
        let tx_id = tx_id_v1(&pd, &rest_digest);
        if is_action_tx_id(&tx_id) {
            println!("Found action tx nonce: {} (iterations: {})", nonce, nonce + 1);
            return payload;
        }
    }
    unreachable!()
}

/// Create mock transactions for testing
fn create_mock_block_txs(block_index: u32) -> Vec<MockTx> {
    let mut txs = Vec::new();

    // Type 1: Regular tx (version 0, random txid that doesn't start with ACTN)
    let mut regular_tx_id = [0u32; 8];
    regular_tx_id[0] = 0xDEADBEEF; // Definitely not ACTN
    regular_tx_id[1] = block_index;
    regular_tx_id[2] = 0x11111111;
    txs.push(MockTx::V0 { tx_id: regular_tx_id });
    println!("Block {}: Added regular tx (v0, non-ACTN prefix)", block_index);

    // Type 2a: Version 0 tx with txid that happens to start with action prefix byte
    // (Guest only checks action prefix for version > 0, so this won't be processed as action)
    let mut fake_actn_v0 = [0u32; 8];
    fake_actn_v0[0] = ACTION_TX_ID_PREFIX as u32; // First byte matches action prefix
    fake_actn_v0[1] = block_index;
    fake_actn_v0[2] = 0x22222222;
    txs.push(MockTx::V0 { tx_id: fake_actn_v0 });
    println!("Block {}: Added fake action tx (v0 with action prefix - not processed)", block_index);

    // Type 2b: Version 1 tx with action prefix but INVALID action discriminator
    // We need to brute-force a nonce that gives action prefix with invalid action
    let invalid_rest_digest = [block_index, 0x33333333, 0, 0, 0, 0, 0, 0];
    for nonce in 0u32.. {
        let payload = VersionedActionRaw {
            action_version: 1,
            action_raw: [255, 0], // Invalid discriminator (only 0 and 1 are valid)
            nonce,
        };
        let payload_words = payload.as_words();
        let pd = payload_digest(payload_words);
        let tx_id = tx_id_v1(&pd, &invalid_rest_digest);
        if is_action_tx_id(&tx_id) {
            txs.push(MockTx::V1 { version: 1, payload, rest_digest: invalid_rest_digest });
            println!("Block {}: Added invalid action tx (v1, action prefix, bad discriminator, nonce={})", block_index, nonce);
            break;
        }
    }

    // Type 3: Valid action tx - different actions per block
    let action = match block_index % 3 {
        0 => Action::Fib(10),      // Fib(10) = 55
        1 => Action::Factorial(5), // 5! = 120
        _ => Action::Fib(15),      // Fib(15) = 610
    };
    let rest_digest = [block_index, 0x44444444, 0, 0, 0, 0, 0, 0];
    let payload = find_action_tx_nonce(action, 1, rest_digest);
    let tx_id = {
        let pd = payload_digest(payload.as_words());
        tx_id_v1(&pd, &rest_digest)
    };
    println!("Block {}: Added valid action tx {:?}, tx_id[0]=0x{:08X}", block_index, action, tx_id[0]);
    txs.push(MockTx::V1 { version: 1, payload, rest_digest });

    txs
}

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

    // --- Initial state ---
    let mut state = State::default();
    let prev_state_hash = state.hash();
    println!("Initial state hash: {}", faster_hex::hex_string(bytemuck::bytes_of(&prev_state_hash)));

    // --- Build chain of 3 blocks with txs, compute seq_commitment chain ---
    let prev_seq_commitment_hash = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
    let prev_seq_commitment = from_bytes(prev_seq_commitment_hash.as_bytes());
    println!("Initial seq_commitment: {}", faster_hex::hex_string(bytemuck::bytes_of(&prev_seq_commitment)));

    let chain_len = 3u32;
    let block_hashes: Vec<Hash> = (1..=chain_len).map(|i| Hash::from_u64_word(i as u64)).collect();

    // Create mock transactions for each block
    println!("\n=== Creating mock transactions ===");
    let block_txs: Vec<Vec<MockTx>> = (0..chain_len).map(create_mock_block_txs).collect();

    // Compute seq_commitment chain and update state
    let mut seq_commit = prev_seq_commitment_hash;
    let mut accessor_map = HashMap::new();

    for (block_idx, (block_hash, txs)) in block_hashes.iter().zip(block_txs.iter()).enumerate() {
        // Compute tx leaf digests for merkle tree
        let tx_digests: Vec<Hash> = txs
            .iter()
            .map(|tx| {
                let tx_id = tx.tx_id();
                let version = tx.version();
                let leaf = seq_commitment_leaf(&tx_id, version);
                Hash::from_bytes(bytemuck::cast_slice(&leaf).try_into().unwrap())
            })
            .collect();

        // Update seq_commitment
        seq_commit = calc_accepted_id_merkle_root(seq_commit, tx_digests.into_iter());
        accessor_map.insert(*block_hash, seq_commit);
        println!("Block {} (hash={:?}): seq_commit = {:?}", block_idx, block_hash, seq_commit);

        // Update state with valid actions
        for tx in txs {
            if tx.is_valid_action() {
                if let MockTx::V1 { payload, .. } = tx {
                    if let Ok(action) = Action::try_from(payload.action_raw) {
                        let output = action.execute();
                        state.add_new_result(action, output);
                        println!("  Executed action {:?} -> output={}", action, output);
                    }
                }
            }
        }
    }

    let block_prove_to = *block_hashes.last().unwrap();
    let new_seq_commitment_hash = seq_commit;
    let new_seq_commitment = from_bytes(new_seq_commitment_hash.as_bytes());

    // Compute new state hash after all actions
    let new_state_hash = state.hash();
    println!("Final state hash: {}", faster_hex::hex_string(bytemuck::bytes_of(&new_state_hash)));
    println!("Final seq_commitment: {}", faster_hex::hex_string(bytemuck::bytes_of(&new_seq_commitment)));

    let public_input = PublicInput { prev_state_hash, prev_seq_commitment };

    // On-chain preimage order: prev_state_hash || prev_seq_commitment || new_state_hash || new_seq_commitment
    let mut journal_preimage = [0u8; 128];
    journal_preimage[0..32].copy_from_slice(bytemuck::bytes_of(&prev_state_hash));
    journal_preimage[32..64].copy_from_slice(bytemuck::bytes_of(&prev_seq_commitment));
    journal_preimage[64..96].copy_from_slice(bytemuck::bytes_of(&new_state_hash));
    journal_preimage[96..128].copy_from_slice(bytemuck::bytes_of(&new_seq_commitment));

    // --- Prove with RISC0 ---
    println!("\n=== Building RISC0 executor environment ===");
    let mut binding = ExecutorEnv::builder();
    let initial_state = State::default(); // Guest receives initial state
    let builder = binding
        .write_slice(core::slice::from_ref(&public_input))
        .write_slice(initial_state.as_word_slice())
        .write_slice(&chain_len.to_le_bytes());

    for txs in &block_txs {
        let tx_count = txs.len() as u32;
        builder.write_slice(&tx_count.to_le_bytes());
        for tx in txs {
            tx.write_to_env(builder);
        }
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

    receipt.verify(ZK_COVENANT_ROLLUP_GUEST_ID).unwrap();

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
