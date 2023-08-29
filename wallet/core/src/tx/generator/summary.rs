use crate::utils::*;
use kaspa_consensus_core::network::NetworkType;
use kaspa_consensus_core::tx::TransactionId;
use std::fmt;

#[derive(Debug, Clone)]
pub struct GeneratorSummary {
    pub network_type: NetworkType,
    pub aggregated_utxos: usize,
    pub aggregated_fees: u64,
    pub number_of_generated_transactions: usize,
    pub final_transaction_amount: Option<u64>,
    pub final_transaction_id: Option<TransactionId>,
}

impl GeneratorSummary {
    pub fn network_type(&self) -> NetworkType {
        self.network_type
    }

    pub fn aggregated_utxos(&self) -> usize {
        self.aggregated_utxos
    }

    pub fn aggregated_fees(&self) -> u64 {
        self.aggregated_fees
    }

    pub fn number_of_generated_transactions(&self) -> usize {
        self.number_of_generated_transactions
    }

    pub fn final_transaction_amount(&self) -> Option<u64> {
        self.final_transaction_amount
    }

    pub fn final_transaction_id(&self) -> Option<TransactionId> {
        self.final_transaction_id
    }
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
                sompi_to_kaspa_string_with_suffix(final_transaction_amount, &self.network_type),
                sompi_to_kaspa_string_with_suffix(self.aggregated_fees, &self.network_type),
                sompi_to_kaspa_string_with_suffix(total, &self.network_type),
                self.aggregated_utxos,
                transactions
            )?;
        } else {
            write!(
                f,
                "Fees: {}  UTXOs: {}  {}",
                sompi_to_kaspa_string_with_suffix(self.aggregated_fees, &self.network_type),
                self.aggregated_utxos,
                transactions
            )?;
        }
        Ok(())
    }
}
