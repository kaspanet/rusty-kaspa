use kaspa_consensus_core::config::params::TESTNET12_PARAMS;
use kaspa_consensus_core::mass::{Mass, MassCalculator};
use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION_POST_COV_HF},
    hashing::sighash::SigHashReusedValuesUnsync,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput,
        UtxoEntry,
    },
};
use kaspa_hashes::Hash;
use kaspa_txscript::{
    caches::Cache, covenants::CovenantsContext, engine_context::EngineContext, seq_commit_accessor::SeqCommitAccessor, EngineFlags,
    TxScriptEngine,
};

/// Create a mock covenant transaction
pub fn make_mock_transaction(lock_time: u64, input_spk: ScriptPublicKey, output_spk: ScriptPublicKey) -> (Transaction, UtxoEntry) {
    let cov_id = Hash::from_bytes([0xFF; 32]);
    let tx = Transaction::new(
        TX_VERSION_POST_COV_HF,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(1), 1), vec![], 10, 115)],
        vec![TransactionOutput::with_covenant(
            SOMPI_PER_KASPA,
            output_spk,
            Some(CovenantBinding { authorizing_input: 0, covenant_id: cov_id }),
        )],
        lock_time,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let utxo = UtxoEntry::new(SOMPI_PER_KASPA, input_spk, 0, false, Some(cov_id));
    (tx, utxo)
}

/// Create a mock covenant transaction with a permission output.
///
/// Output 0: state continuation (covenant-bound), Output 1: permission (covenant-bound).
pub fn make_mock_transaction_with_permission(
    lock_time: u64,
    input_spk: ScriptPublicKey,
    output_spk: ScriptPublicKey,
    permission_spk: ScriptPublicKey,
) -> (Transaction, UtxoEntry) {
    let cov_id = Hash::from_bytes([0xFF; 32]);
    let tx = Transaction::new(
        TX_VERSION_POST_COV_HF,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(1), 1), vec![], 10, 115)],
        vec![
            TransactionOutput::with_covenant(
                SOMPI_PER_KASPA,
                output_spk,
                Some(CovenantBinding { authorizing_input: 0, covenant_id: cov_id }),
            ),
            TransactionOutput::with_covenant(
                SOMPI_PER_KASPA,
                permission_spk,
                Some(CovenantBinding { authorizing_input: 0, covenant_id: cov_id }),
            ),
        ],
        lock_time,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let utxo = UtxoEntry::new(2 * SOMPI_PER_KASPA, input_spk, 0, false, Some(cov_id));
    (tx, utxo)
}

/// Verify a transaction using the script engine
pub fn verify_tx(tx: &Transaction, utxo: &UtxoEntry, accessor: &dyn SeqCommitAccessor) {
    let calc = MassCalculator::new_with_consensus_params(&TESTNET12_PARAMS);
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, vec![utxo.clone()]);
    let ctx_mass = calc.calc_contextual_masses(&populated).unwrap();
    let non_ctx_mass = calc.calc_non_contextual_masses(populated.tx);
    const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 1_000_000; // TODO(covpp-mainnet)
    let norm_mass = Mass::new(non_ctx_mass, ctx_mass).normalized_max(&TESTNET12_PARAMS.block_mass_limits.cofactors());
    assert!(dbg!(norm_mass) < MAXIMUM_STANDARD_TRANSACTION_MASS, "transaction mass is larger than max allowed size of 1000000");

    let cov_ctx = CovenantsContext::from_tx(&populated).unwrap();
    let exec_ctx =
        EngineContext::new(&sig_cache).with_reused(&reused_values).with_seq_commit_accessor(accessor).with_covenants_ctx(&cov_ctx);

    let mut vm = TxScriptEngine::from_transaction_input(&populated, &tx.inputs[0], 0, utxo, exec_ctx, flags);
    vm.execute().unwrap();
}

/// Multi-input/output mock transaction for permission/delegate testing.
pub fn make_multi_input_mock_transaction(
    inputs_spk: Vec<(u64, ScriptPublicKey, Option<Hash>)>,
    outputs: Vec<(u64, ScriptPublicKey, Option<CovenantBinding>)>,
) -> (Transaction, Vec<UtxoEntry>) {
    let tx = Transaction::new(
        TX_VERSION_POST_COV_HF,
        inputs_spk
            .iter()
            .enumerate()
            .map(|(i, _)| TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(i as u64 + 1), i as u32), vec![], 10, 1))
            .collect(),
        outputs.into_iter().map(|(value, spk, covenant)| TransactionOutput::with_covenant(value, spk, covenant)).collect(),
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    let utxos: Vec<UtxoEntry> =
        inputs_spk.into_iter().map(|(amount, spk, cov_id)| UtxoEntry::new(amount, spk, 0, false, cov_id)).collect();
    (tx, utxos)
}

/// Verify a specific input of a transaction. Panics on failure.
pub fn verify_tx_input(tx: &Transaction, utxos: &[UtxoEntry], input_idx: usize, accessor: &dyn SeqCommitAccessor) {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, utxos.to_vec());
    let cov_ctx = CovenantsContext::from_tx(&populated).unwrap();
    let exec_ctx =
        EngineContext::new(&sig_cache).with_reused(&reused_values).with_seq_commit_accessor(accessor).with_covenants_ctx(&cov_ctx);

    let mut vm =
        TxScriptEngine::from_transaction_input(&populated, &tx.inputs[input_idx], input_idx, &utxos[input_idx], exec_ctx, flags);
    vm.execute().unwrap();
}

/// Like verify_tx_input but returns Result for error testing.
pub fn try_verify_tx_input(
    tx: &Transaction,
    utxos: &[UtxoEntry],
    input_idx: usize,
    accessor: &dyn SeqCommitAccessor,
) -> Result<(), String> {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, utxos.to_vec());
    let cov_ctx = CovenantsContext::from_tx(&populated).unwrap();
    let exec_ctx =
        EngineContext::new(&sig_cache).with_reused(&reused_values).with_seq_commit_accessor(accessor).with_covenants_ctx(&cov_ctx);

    let mut vm =
        TxScriptEngine::from_transaction_input(&populated, &tx.inputs[input_idx], input_idx, &utxos[input_idx], exec_ctx, flags);
    vm.execute().map_err(|e| format!("{e}"))
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{
        hashing::covenant_id::covenant_id as compute_genesis_covenant_id,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{
            CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry,
        },
    };
    use kaspa_hashes::Hash;
    use kaspa_txscript::covenants::CovenantsContext;

    use super::TX_VERSION_POST_COV_HF;

    fn dummy_spk() -> ScriptPublicKey {
        ScriptPublicKey::default()
    }

    /// Build a minimal single-input transaction and finalize it.
    fn make_tx(outpoint: TransactionOutpoint, outputs: Vec<TransactionOutput>, version: u16) -> Transaction {
        let input = TransactionInput::new(outpoint, vec![], 0, 0);
        let mut tx = Transaction::new(version, vec![input], outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
        tx.finalize();
        tx
    }

    // ── Deploy tx covenant ID ────────────────────────────────────────────────

    /// The deploy tx's output must carry a CovenantBinding whose covenant_id equals the
    /// genesis hash of (deploy_input_outpoint, deploy_outputs).  This test verifies that
    /// CovenantsContext::from_tx accepts the transaction and treats it as genesis.
    #[test]
    fn test_deploy_tx_genesis_covenant_id_is_accepted() {
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(42), 0);

        // Build plain output first (covenant binding field excluded from hash — no circularity).
        let plain_output = TransactionOutput::new(1_000_000, dummy_spk());
        let genesis_id = compute_genesis_covenant_id(outpoint, std::iter::once((0u32, &plain_output)));

        // Build deploy tx with genesis covenant binding on the output.
        let output = TransactionOutput::with_covenant(
            1_000_000,
            dummy_spk(),
            Some(CovenantBinding { covenant_id: genesis_id, authorizing_input: 0 }),
        );
        let tx = make_tx(outpoint, vec![output], 0);
        let utxo = UtxoEntry::new(1_000_000, dummy_spk(), 0, false, None);
        let populated = PopulatedTransaction::new(&tx, vec![utxo]);

        // Genesis validation must pass (the computed id matches).
        let ctx = CovenantsContext::from_tx(&populated).expect("deploy tx genesis validation failed");

        // Genesis outputs do NOT populate script-engine contexts.
        assert!(ctx.input_ctxs.is_empty(), "genesis should not add input ctx");
        assert!(ctx.shared_ctxs.is_empty(), "genesis should not add shared ctx");
    }

    /// Sanity-check: if a deploy tx output uses a *wrong* covenant_id, from_tx must reject it.
    #[test]
    fn test_deploy_tx_wrong_covenant_id_is_rejected() {
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(42), 0);
        let wrong_id = Hash::from_bytes([0xAB; 32]);

        let output = TransactionOutput::with_covenant(
            1_000_000,
            dummy_spk(),
            Some(CovenantBinding { covenant_id: wrong_id, authorizing_input: 0 }),
        );
        let tx = make_tx(outpoint, vec![output], 0);
        let utxo = UtxoEntry::new(1_000_000, dummy_spk(), 0, false, None);
        let populated = PopulatedTransaction::new(&tx, vec![utxo]);

        let result = CovenantsContext::from_tx(&populated);
        assert!(result.is_err(), "wrong covenant_id should be rejected");
    }

    // ── Proof tx covenant continuity ─────────────────────────────────────────

    /// A proof tx spending the deploy UTXO must be a *continuation* (input covenant_id ==
    /// output covenant_id).  This test builds the deploy UTXO with on_chain_covenant_id set,
    /// then verifies that the proof tx is accepted without triggering genesis validation.
    #[test]
    fn test_proof_tx_is_continuation_of_deploy_utxo() {
        // Simulate the genesis covenant_id that the deploy tx produced.
        let deploy_outpoint = TransactionOutpoint::new(Hash::from_u64_word(42), 0);
        let plain = TransactionOutput::new(1_000_000, dummy_spk());
        let genesis_id = compute_genesis_covenant_id(deploy_outpoint, std::iter::once((0u32, &plain)));

        // The deploy UTXO carries covenant_id = genesis_id (set by the node when the deploy
        // tx output had a CovenantBinding with that id).
        let proof_input_outpoint = TransactionOutpoint::new(Hash::from_u64_word(100), 0);
        let deploy_utxo = UtxoEntry::new(997_000, dummy_spk(), 0, false, Some(genesis_id));

        // Proof tx: single input (deploy UTXO), single output with same covenant_id.
        let output = TransactionOutput::with_covenant(
            994_000, // value minus fee
            dummy_spk(),
            Some(CovenantBinding { covenant_id: genesis_id, authorizing_input: 0 }),
        );
        let tx = make_tx(proof_input_outpoint, vec![output], TX_VERSION_POST_COV_HF);
        let populated = PopulatedTransaction::new(&tx, vec![deploy_utxo]);

        // Must succeed: continuation case (no genesis validation triggered).
        let ctx = CovenantsContext::from_tx(&populated).expect("proof tx continuation validation failed");

        // The covenant input must appear in shared_ctxs and must authorize output 0.
        assert!(!ctx.shared_ctxs.is_empty(), "shared context must exist for covenant input");
        // input_ctxs[0].auth_outputs must be [0]
        let input_ctx = ctx.input_ctxs.get(&0).expect("input 0 must have an input ctx");
        assert_eq!(input_ctx.auth_outputs, vec![0], "input 0 must authorize output 0");
    }

    // ── Succinct proof verification with captured data ─────────────────────

    /// Fast test that verifies a succinct proof using hardcoded data captured from a real
    /// proving run.  No ZK prover or kaspad node required — runs in ~0s.
    ///
    /// To re-capture data: run `capture_succinct_proof_data` (ignored, slow):
    /// `cargo test --release capture_succinct_proof_data -- --ignored --nocapture`
    #[test]
    fn test_succinct_proof_verification_with_captured_data() {
        use std::collections::HashMap;

        use kaspa_txscript::{pay_to_script_hash_script, script_builder::ScriptBuilder, zk_precompiles::tags::ZkTag};
        use zk_covenant_rollup_methods::ZK_COVENANT_ROLLUP_GUEST_ID;

        use crate::mock_chain::MockSeqCommitAccessor;
        use crate::redeem::build_redeem_script;

        // ── Hardcoded proof data (captured from a real proving run) ──
        // Seal is large (~222KB), stored as binary file; small fields inlined as hex.
        // To re-capture: run `capture_succinct_proof_data` (ignored, slow).
        let seal_bytes: &[u8] = include_bytes!("../testdata/captured_seal.bin");

        const CLAIM_HEX: &str = "66d77a344274c7bf7215936058f4620be4d477003f6d01bf86b843c034358ced";
        const HASHFN: &str = "poseidon2";
        const CONTROL_INDEX: u32 = 10;
        const CONTROL_DIGESTS_HEX: &str = "fd84d83092a1e1244d423a26d89c892ab098b467c6d82229912deb26e37d2562dafe25646d370c28fe472176911d2c541ba6e243b1d9150fd67d6a055116f1690bb1e41c4f4912522725016e09358171398a9a6d44fe5d5c648eb8226e46ed50c64e2b5c7ffa46692f5939054290d36dd4b84477dbb78a3d3aaba251d43caf24977f9e2868d664458077ac35fa9050290c7db016c2750620c362da3c275cab67f765ab6e0cf5dc55c11d65688af0fe1428afc359c08b1656bbc4ba6b54c9746cc6b87a237165c549ef7ac614d762ec1ce4b97441c9bfef6fd8ac90378170d8162be97040fd0b390959c33114712a436382b2cd419665ee2fe801c158a9bbb155";
        const BLOCK_PROVE_TO_HEX: &str = "0000000000000000000000000000000000000000000000000300000000000000";
        const NEW_STATE_HEX: &str = "47345fa0e4e721619cdd7328fdfde5dd2d5a66a6a1a9d93d997d5f5637bda86e";
        const NEW_SEQ_HEX: &str = "54c51a0910cbfa7c0b41ab0bfcf512a15919c718995f1c5fe5d437252df4d448";
        const PREV_STATE_HEX: &str = "25f706375943a1eadc748b295b87372835b518300f9df52f95f2d980a2cd6e32";
        const PREV_SEQ_HEX: &str = "73ccb9fdf73a01aa761c348c706b7b6cc9551fbba0ea00e1d84d8664cb81af90";
        const COVENANT_ID_HEX: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        // Permission SPK (P2SH of permission redeem script) — present when exits occurred.
        // Empty string means no exits / no permission output.
        const PERM_SPK_HEX: &str = "aa20c1208076e385c63701b353c9c7c39cc669bcce3eacf9f818f589045b1be1298887";

        // ── Decode hex constants ──
        fn hex(s: &str) -> Vec<u8> {
            let mut out = vec![0u8; s.len() / 2];
            faster_hex::hex_decode(s.as_bytes(), &mut out).expect("invalid hex");
            out
        }

        let claim_bytes = hex(CLAIM_HEX);
        let hashfn_byte: Vec<u8> = vec![zk_covenant_common::hashfn_str_to_id(HASHFN).expect("invalid hashfn")];
        let control_index_bytes: Vec<u8> = CONTROL_INDEX.to_le_bytes().to_vec();
        let control_digests_bytes = hex(CONTROL_DIGESTS_HEX);
        let block_prove_to_bytes = hex(BLOCK_PROVE_TO_HEX);
        let new_state_bytes = hex(NEW_STATE_HEX);
        let new_seq_bytes = hex(NEW_SEQ_HEX);
        let prev_state_bytes = hex(PREV_STATE_HEX);
        let prev_seq_bytes = hex(PREV_SEQ_HEX);
        let covenant_id_bytes = hex(COVENANT_ID_HEX);

        let block_prove_to = Hash::from_slice(&block_prove_to_bytes);
        let new_state_hash: [u32; 8] = bytemuck::pod_read_unaligned(&new_state_bytes);
        let new_seq_commitment: [u32; 8] = bytemuck::pod_read_unaligned(&new_seq_bytes);
        let prev_state_hash: [u32; 8] = bytemuck::pod_read_unaligned(&prev_state_bytes);
        let prev_seq_commitment: [u32; 8] = bytemuck::pod_read_unaligned(&prev_seq_bytes);
        let covenant_id = Hash::from_slice(&covenant_id_bytes);

        let program_id: [u8; 32] = bytemuck::cast(ZK_COVENANT_ROLLUP_GUEST_ID);
        let zk_tag = ZkTag::R0Succinct;

        // ── Build redeem scripts (convergence loop) ──
        let mut computed_len: i64 = 75;
        loop {
            let script = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, &program_id, &zk_tag);
            let new_len = script.len() as i64;
            if new_len == computed_len {
                break;
            }
            computed_len = new_len;
        }

        let input_redeem = build_redeem_script(prev_state_hash, prev_seq_commitment, computed_len, &program_id, &zk_tag);
        let output_redeem = build_redeem_script(new_state_hash, new_seq_commitment, computed_len, &program_id, &zk_tag);

        // ── Build mock transaction with the real covenant_id ──
        let mut outputs = vec![TransactionOutput::with_covenant(
            super::SOMPI_PER_KASPA,
            pay_to_script_hash_script(&output_redeem),
            Some(CovenantBinding { authorizing_input: 0, covenant_id }),
        )];
        // Add permission output if exits occurred
        if !PERM_SPK_HEX.is_empty() {
            let perm_spk_bytes = hex(PERM_SPK_HEX);
            let perm_spk = ScriptPublicKey::new(0, perm_spk_bytes.into());
            outputs.push(TransactionOutput::with_covenant(
                super::SOMPI_PER_KASPA,
                perm_spk,
                Some(CovenantBinding { authorizing_input: 0, covenant_id }),
            ));
        }
        let mut tx = Transaction::new(
            super::TX_VERSION_POST_COV_HF,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(1), 1), vec![], 10, 115)],
            outputs,
            0,
            super::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let input_value = super::SOMPI_PER_KASPA * tx.outputs.len() as u64;
        let utxos = vec![UtxoEntry::new(input_value, pay_to_script_hash_script(&input_redeem), 0, false, Some(covenant_id))];

        // ── Assemble sig_script from hardcoded proof components ──
        tx.inputs[0].signature_script = ScriptBuilder::new()
            .add_data(seal_bytes)
            .unwrap()
            .add_data(&claim_bytes)
            .unwrap()
            .add_data(&hashfn_byte)
            .unwrap()
            .add_data(&control_index_bytes)
            .unwrap()
            .add_data(&control_digests_bytes)
            .unwrap()
            .add_data(block_prove_to.as_bytes().as_slice())
            .unwrap()
            .add_data(bytemuck::bytes_of(&new_state_hash))
            .unwrap()
            .add_data(&input_redeem)
            .unwrap()
            .drain();

        // ── Mock accessor: block_prove_to → new_seq_commitment as Hash ──
        let new_seq_as_hash = Hash::from_slice(bytemuck::bytes_of(&new_seq_commitment));
        let mut map = HashMap::new();
        map.insert(block_prove_to, new_seq_as_hash);
        let accessor = MockSeqCommitAccessor(map);

        // ── Verify — no real node, no ZK prover ──
        super::verify_tx_input(&tx, &utxos, 0, &accessor);
    }

    /// Capture proof data for `test_succinct_proof_verification_with_captured_data`.
    ///
    /// Runs the mock chain and proves with succinct proofs, then writes captured data:
    /// - `testdata/captured_seal.bin` (binary seal)
    /// - Hex constants printed to stderr for updating the hardcoded test
    ///
    /// Run with: `cargo test --release capture_succinct_proof_data -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn capture_succinct_proof_data() {
        use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv, ProverOpts};
        use zk_covenant_rollup_core::PublicInput;
        use zk_covenant_rollup_methods::{ZK_COVENANT_ROLLUP_GUEST_ELF, ZK_COVENANT_ROLLUP_GUEST_ID};

        use crate::mock_chain::{build_initial_smt, build_mock_chain, calc_accepted_id_merkle_root, from_bytes};

        // Build initial state
        let initial_smt = build_initial_smt();
        let prev_state_hash = initial_smt.root();
        let prev_seq_commit_hash = calc_accepted_id_merkle_root(Hash::default(), std::iter::empty());
        let prev_seq_commitment = from_bytes(prev_seq_commit_hash.as_bytes());

        // Build mock chain
        let chain = build_mock_chain(prev_seq_commit_hash, &[0xFF; 32], 0);
        let new_state_hash = chain.final_state_root;
        let new_seq_commitment = from_bytes(chain.final_seq_commit.as_bytes());

        let covenant_id = from_bytes([0xFF; 32]);
        let public_input = PublicInput { prev_state_hash, prev_seq_commitment, covenant_id };

        // Build executor env
        let env = {
            let mut binding = ExecutorEnv::builder();
            let builder =
                binding.write_slice(core::slice::from_ref(&public_input)).write_slice(&(chain.block_txs.len() as u32).to_le_bytes());
            for txs in &chain.block_txs {
                builder.write_slice(&(txs.len() as u32).to_le_bytes());
                for tx in txs {
                    tx.write_to_env(builder);
                }
            }
            if let Some(len) = chain.perm_redeem_script_len {
                builder.write_slice(&(len as u32).to_le_bytes());
            }
            builder.build().unwrap()
        };

        // Prove
        eprintln!("Proving (succinct)...");
        let prover = default_prover();
        let info = prover.prove_with_opts(env, ZK_COVENANT_ROLLUP_GUEST_ELF, &ProverOpts::succinct()).unwrap();
        let receipt = info.receipt;
        receipt.verify(ZK_COVENANT_ROLLUP_GUEST_ID).unwrap();
        eprintln!("Proof verified!");

        let succinct = receipt.inner.succinct().expect("not a succinct receipt");
        let block_prove_to = *chain.block_hashes.last().unwrap();

        // Write seal binary
        let seal_bytes: Vec<u8> = succinct.seal.iter().flat_map(|w| w.to_le_bytes()).collect();
        let seal_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join("captured_seal.bin");
        std::fs::create_dir_all(seal_path.parent().unwrap()).unwrap();
        std::fs::write(&seal_path, &seal_bytes).unwrap();
        eprintln!("Wrote seal to {}", seal_path.display());

        // Print hex constants
        let claim_hex = faster_hex::hex_string(succinct.claim.digest().as_bytes());
        let control_index = succinct.control_inclusion_proof.index;
        let control_digests_hex = faster_hex::hex_string(
            &succinct.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes().iter().copied()).collect::<Vec<u8>>(),
        );
        let block_prove_to_hex = faster_hex::hex_string(block_prove_to.as_bytes().as_slice());
        let new_state_hex = faster_hex::hex_string(bytemuck::bytes_of(&new_state_hash));
        let new_seq_hex = faster_hex::hex_string(bytemuck::bytes_of(&new_seq_commitment));
        let prev_state_hex = faster_hex::hex_string(bytemuck::bytes_of(&prev_state_hash));
        let prev_seq_hex = faster_hex::hex_string(bytemuck::bytes_of(&prev_seq_commitment));
        let covenant_id_hex = faster_hex::hex_string(bytemuck::bytes_of(&covenant_id));

        eprintln!("\n=== CAPTURED PROOF DATA ===");
        eprintln!("CLAIM_HEX: \"{}\"", claim_hex);
        eprintln!("HASHFN: \"{}\"", succinct.hashfn);
        eprintln!("CONTROL_INDEX: {}", control_index);
        eprintln!("CONTROL_DIGESTS_HEX: \"{}\"", control_digests_hex);
        eprintln!("BLOCK_PROVE_TO_HEX: \"{}\"", block_prove_to_hex);
        eprintln!("NEW_STATE_HEX: \"{}\"", new_state_hex);
        eprintln!("NEW_SEQ_HEX: \"{}\"", new_seq_hex);
        eprintln!("PREV_STATE_HEX: \"{}\"", prev_state_hex);
        eprintln!("PREV_SEQ_HEX: \"{}\"", prev_seq_hex);
        eprintln!("COVENANT_ID_HEX: \"{}\"", covenant_id_hex);
        if let Some(ref perm_redeem) = chain.permission_redeem {
            use kaspa_txscript::pay_to_script_hash_script;
            let perm_spk = pay_to_script_hash_script(perm_redeem);
            eprintln!("PERM_SPK_HEX: \"{}\"", faster_hex::hex_string(perm_spk.script()));
        } else {
            eprintln!("PERM_SPK_HEX: (none — no exits)");
        }
        eprintln!("=== END CAPTURED PROOF DATA ===");
    }

    /// Regression: the *old* bug — deploy output had no CovenantBinding, so the deploy
    /// UTXO had covenant_id = None, and the proof tx's output became a *genesis* with the
    /// wrong covenant_id, causing WrongGenesisCovenantId.
    #[test]
    fn test_proof_tx_fails_when_deploy_utxo_has_no_covenant_id() {
        // Deploy UTXO without covenant_id (old behaviour before the fix).
        let proof_input_outpoint = TransactionOutpoint::new(Hash::from_u64_word(100), 0);
        let deploy_utxo = UtxoEntry::new(997_000, dummy_spk(), 0, false, None);

        // Any covenant_id on the proof output — does not matter what value.
        let arbitrary_id = Hash::from_bytes([0xCD; 32]);
        let output = TransactionOutput::with_covenant(
            994_000,
            dummy_spk(),
            Some(CovenantBinding { covenant_id: arbitrary_id, authorizing_input: 0 }),
        );
        let tx = make_tx(proof_input_outpoint, vec![output], TX_VERSION_POST_COV_HF);
        let populated = PopulatedTransaction::new(&tx, vec![deploy_utxo]);

        // Must fail: genesis case with wrong hash.
        let result = CovenantsContext::from_tx(&populated);
        assert!(result.is_err(), "expected genesis covenant_id validation to fail");
    }
}
