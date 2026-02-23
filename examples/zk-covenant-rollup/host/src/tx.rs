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
