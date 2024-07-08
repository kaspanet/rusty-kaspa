use std::collections::HashMap;

use crate::tx::TransactionId;

/// A struct provided to consensus for transaction validation processing calls
#[derive(Clone, Debug, Default)]
pub struct TransactionValidationArgs {
    /// Optional fee/mass threshold above which a bound transaction in not rejected
    pub fee_per_mass_threshold: Option<f64>,
}

impl TransactionValidationArgs {
    pub fn new(fee_per_mass_threshold: Option<f64>) -> Self {
        Self { fee_per_mass_threshold }
    }
}

/// A struct provided to consensus for transactions validation processing calls
pub struct TransactionBatchValidationArgs {
    tx_args: HashMap<TransactionId, TransactionValidationArgs>,
}

impl TransactionBatchValidationArgs {
    const DEFAULT_ARGS: TransactionValidationArgs = TransactionValidationArgs { fee_per_mass_threshold: None };

    pub fn new() -> Self {
        Self { tx_args: HashMap::new() }
    }

    /// Set some fee/mass threshold for transaction `transaction_id`.
    pub fn set_fee_per_mass_threshold(&mut self, transaction_id: TransactionId, threshold: f64) {
        self.tx_args
            .entry(transaction_id)
            .and_modify(|x| x.fee_per_mass_threshold = Some(threshold))
            .or_insert(TransactionValidationArgs::new(Some(threshold)));
    }

    pub fn get(&self, transaction_id: &TransactionId) -> &TransactionValidationArgs {
        self.tx_args.get(transaction_id).unwrap_or(&Self::DEFAULT_ARGS)
    }
}

impl Default for TransactionBatchValidationArgs {
    fn default() -> Self {
        Self::new()
    }
}
