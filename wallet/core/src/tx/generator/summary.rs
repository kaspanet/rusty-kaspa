use crate::network::NetworkId;
use crate::utils::*;
use kaspa_consensus_core::tx::TransactionId;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GeneratorSummary {
    pub network_id: NetworkId,
    pub aggregated_utxos: usize,
    pub aggregated_fees: u64,
    pub number_of_generated_transactions: usize,
    pub final_transaction_amount: Option<u64>,
    pub final_transaction_id: Option<TransactionId>,
}

impl fmt::Display for GeneratorSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let transactions = if self.number_of_generated_transactions == 1 {
            "".to_string()
        } else {
            format!("Batch Transactions: {}", self.number_of_generated_transactions)
        };

        if let Some(final_transaction_amount) = self.final_transaction_amount {
            let total = final_transaction_amount + self.aggregated_fees;
            write!(
                f,
                "Amount: {}  Fees: {}  Total: {}  UTXOs: {}  {}",
                sompi_to_kaspa_string_with_suffix(final_transaction_amount, &self.network_id.into()),
                sompi_to_kaspa_string_with_suffix(self.aggregated_fees, &self.network_id.into()),
                sompi_to_kaspa_string_with_suffix(total, &self.network_id.into()),
                self.aggregated_utxos,
                transactions
            )?;
        } else {
            write!(
                f,
                "Fees: {}  UTXOs: {}  {}",
                sompi_to_kaspa_string_with_suffix(self.aggregated_fees, &self.network_id.into()),
                self.aggregated_utxos,
                transactions
            )?;
        }
        Ok(())
    }
}
