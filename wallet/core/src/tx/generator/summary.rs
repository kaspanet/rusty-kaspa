use kaspa_consensus_core::tx::TransactionId;

#[derive(Debug, Clone)]
pub struct GeneratorSummary {
    pub aggregated_utxos: usize,
    pub aggregated_fees: u64,
    pub number_of_generated_transactions: usize,
    pub final_transaction_id: Option<TransactionId>,
}
