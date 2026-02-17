use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
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
        TX_VERSION + 1,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(1), 1), vec![], 10, u8::MAX)],
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
    let utxo = UtxoEntry::new(0, input_spk, 0, false, Some(cov_id));
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
        TX_VERSION + 1,
        vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(1), 1), vec![], 10, u8::MAX)],
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
    let utxo = UtxoEntry::new(0, input_spk, 0, false, Some(cov_id));
    (tx, utxo)
}

/// Verify a transaction using the script engine
pub fn verify_tx(tx: &Transaction, utxo: &UtxoEntry, accessor: &dyn SeqCommitAccessor) {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, vec![utxo.clone()]);
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
        TX_VERSION + 1,
        inputs_spk
            .iter()
            .enumerate()
            .map(|(i, _)| {
                TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(i as u64 + 1), i as u32), vec![], 10, u8::MAX)
            })
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
