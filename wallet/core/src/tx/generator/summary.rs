use crate::utils::*;
use kaspa_consensus_core::tx::TransactionId;
use crate::network::NetworkId;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GeneratorSummary {
    network_id : NetworkId,
    pub aggregated_utxos: usize,
    pub aggregated_fees: u64,
    pub number_of_generated_transactions: usize,
    pub final_transaction_id: Option<TransactionId>,
}

impl fmt::Display for GeneratorSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "UTXOs: {} Fees: {} Transactions: {}",
            self.aggregated_utxos,
            sompi_to_kaspa_string_with_suffix(self.aggregated_fees,&self.network_id.into()),
            self.number_of_generated_transactions
        )?;
        Ok(())
    }
}
