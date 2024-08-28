use std::collections::HashMap;

use crate::tx::TransactionId;

/// A struct provided to consensus for transaction validation processing calls
#[derive(Clone, Debug, Default)]
pub struct TransactionValidationArgs {
    /// Optional fee/mass threshold above which a bound transaction in not rejected
    pub feerate_threshold: Option<f64>,
}

impl TransactionValidationArgs {
    pub fn new(feerate_threshold: Option<f64>) -> Self {
        Self { feerate_threshold }
    }
}

/// A struct provided to consensus for transactions validation batch processing calls
pub struct TransactionValidationBatchArgs {
    tx_args: HashMap<TransactionId, TransactionValidationArgs>,
}

impl TransactionValidationBatchArgs {
    const DEFAULT_ARGS: TransactionValidationArgs = TransactionValidationArgs { feerate_threshold: None };

    pub fn new() -> Self {
        Self { tx_args: HashMap::new() }
    }

    /// Set some fee/mass threshold for transaction `transaction_id`.
    pub fn set_feerate_threshold(&mut self, transaction_id: TransactionId, feerate_threshold: f64) {
        self.tx_args
            .entry(transaction_id)
            .and_modify(|x| x.feerate_threshold = Some(feerate_threshold))
            .or_insert(TransactionValidationArgs::new(Some(feerate_threshold)));
    }

    pub fn get(&self, transaction_id: &TransactionId) -> &TransactionValidationArgs {
        self.tx_args.get(transaction_id).unwrap_or(&Self::DEFAULT_ARGS)
    }
}

impl Default for TransactionValidationBatchArgs {
    fn default() -> Self {
        Self::new()
    }
}
