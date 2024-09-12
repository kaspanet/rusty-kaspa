//!
//! [`GeneratorSummary`] is a struct that holds the summary
//! of a [`Generator`](crate::tx::Generator) output after transaction generation.
//! The summary includes total amount, total fees consumed,
//! total UTXOs consumed etc.
//!

use crate::utils::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::tx::TransactionId;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct GeneratorSummary {
    pub network_id: NetworkId,
    pub aggregated_utxos: usize,
    pub aggregate_fees: u64,
    pub aggregate_mass: u64,
    pub number_of_generated_transactions: usize,
    pub number_of_generated_stages: usize,
    pub final_transaction_amount: Option<u64>,
    pub final_transaction_id: Option<TransactionId>,
}

impl GeneratorSummary {
    pub fn new(network_id: NetworkId) -> Self {
        Self {
            network_id,
            aggregated_utxos: 0,
            aggregate_fees: 0,
            aggregate_mass: 0,
            number_of_generated_transactions: 0,
            number_of_generated_stages: 0,
            final_transaction_amount: None,
            final_transaction_id: None,
        }
    }

    pub fn network_type(&self) -> NetworkType {
        self.network_id.into()
    }

    pub fn network_id(&self) -> NetworkId {
        self.network_id
    }

    pub fn aggregated_utxos(&self) -> usize {
        self.aggregated_utxos
    }

    pub fn aggregate_mass(&self) -> u64 {
        self.aggregate_mass
    }

    pub fn aggregate_fees(&self) -> u64 {
        self.aggregate_fees
    }

    pub fn number_of_generated_transactions(&self) -> usize {
        self.number_of_generated_transactions
    }

    pub fn number_of_generated_stages(&self) -> usize {
        self.number_of_generated_stages
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
            let total = final_transaction_amount + self.aggregate_fees;
            write!(
                f,
                "Amount: {}  Fees: {}  Total: {}  UTXOs: {}  {}",
                sompi_to_kaspa_string_with_suffix(final_transaction_amount, &self.network_id),
                sompi_to_kaspa_string_with_suffix(self.aggregate_fees, &self.network_id),
                sompi_to_kaspa_string_with_suffix(total, &self.network_id),
                self.aggregated_utxos,
                transactions
            )?;
        } else {
            write!(
                f,
                "Fees: {}  UTXOs: {}  {}",
                sompi_to_kaspa_string_with_suffix(self.aggregate_fees, &self.network_id),
                self.aggregated_utxos,
                transactions
            )?;
        }
        Ok(())
    }
}
