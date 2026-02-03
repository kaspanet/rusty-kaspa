use kaspa_consensus_core::{
    constants::{SOMPI_PER_KASPA, TX_VERSION},
    hashing::sighash::SigHashReusedValuesUnsync,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{
        CovenantBinding, PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput,
        TransactionOutpoint, TransactionOutput, UtxoEntry,
    },
};
use kaspa_hashes::Hash;
use kaspa_txscript::{
    caches::Cache, covenants::CovenantsContext, engine_context::EngineContext,
    seq_commit_accessor::SeqCommitAccessor, EngineFlags, TxScriptEngine,
};

/// Create a mock covenant transaction
pub fn make_mock_transaction(
    lock_time: u64,
    input_spk: ScriptPublicKey,
    output_spk: ScriptPublicKey,
) -> (Transaction, UtxoEntry) {
    let cov_id = Hash::from_bytes([0xFF; 32]);
    let tx = Transaction::new(
        TX_VERSION + 1,
        vec![TransactionInput::new(
            TransactionOutpoint::new(Hash::from_u64_word(1), 1),
            vec![],
            10,
            u8::MAX,
        )],
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

/// Verify a transaction using the script engine
pub fn verify_tx(tx: &Transaction, utxo: &UtxoEntry, accessor: &dyn SeqCommitAccessor) {
    let sig_cache = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let flags = EngineFlags { covenants_enabled: true };

    let populated = PopulatedTransaction::new(tx, vec![utxo.clone()]);
    let cov_ctx = CovenantsContext::from_tx(&populated).unwrap();
    let exec_ctx = EngineContext::new(&sig_cache)
        .with_reused(&reused_values)
        .with_seq_commit_accessor(accessor)
        .with_covenants_ctx(&cov_ctx);

    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[0],
        0,
        utxo,
        exec_ctx,
        flags,
    );
    vm.execute().unwrap();
}
