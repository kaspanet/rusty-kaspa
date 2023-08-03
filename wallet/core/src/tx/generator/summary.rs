use crate::utils::*;
use kaspa_consensus_core::tx::TransactionId;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GeneratorSummary {
    pub aggregated_utxos: usize,
    pub aggregated_fees: u64,
    pub number_of_generated_transactions: usize,
    pub final_transaction_id: Option<TransactionId>,
}

impl fmt::Display for GeneratorSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "UTXOs: {} Fees: {} Transactions: {}",
            self.aggregated_utxos,
            sompi_to_kaspa_string(self.aggregated_fees),
            self.number_of_generated_transactions
        )?;
        Ok(())
    }
}
