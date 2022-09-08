use crate::constants::TX_VERSION;
use consensus_core::tx::Transaction;
use std::collections::HashSet;

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};

impl TransactionValidator {
    pub fn validate_tx_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_inputs_in_isolation(tx)?;
        self.check_transaction_outputs_in_isolation(tx)?;

        check_duplicate_transaction_inputs(tx)?;
        check_gas(tx)?;
        check_transaction_payload(tx)?;
        check_transaction_version(tx)
    }

    fn check_transaction_inputs_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_inputs_count(tx)?;
        self.check_transaction_signature_scripts(tx)
    }

    fn check_transaction_outputs_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_outputs_count(tx)?;
        self.check_transaction_script_public_keys(tx)
    }

    fn check_transaction_outputs_count(&self, tx: &Transaction) -> TxResult<()> {
        if tx.outputs.len() > self.max_tx_outputs {
            return Err(TxRuleError::TooManyInputs(tx.inputs.len(), self.max_tx_inputs));
        }

        Ok(())
    }

    fn check_transaction_inputs_count(&self, tx: &Transaction) -> TxResult<()> {
        if !tx.is_coinbase() && tx.inputs.is_empty() {
            return Err(TxRuleError::NoTxInputs);
        }

        if tx.inputs.len() > self.max_tx_inputs {
            return Err(TxRuleError::TooManyInputs(tx.inputs.len(), self.max_tx_inputs));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_signature_scripts(&self, tx: &Transaction) -> TxResult<()> {
        if let Some(i) = tx
            .inputs
            .iter()
            .position(|input| input.signature_script.len() > self.max_signature_script_len)
        {
            return Err(TxRuleError::TooBigSignatureScript(i, self.max_signature_script_len));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_script_public_keys(&self, tx: &Transaction) -> TxResult<()> {
        if let Some(i) = tx
            .outputs
            .iter()
            .position(|input| input.script_public_key.script.len() > self.max_script_public_key_len)
        {
            return Err(TxRuleError::TooBigScriptPublicKey(i, self.max_script_public_key_len));
        }

        Ok(())
    }
}

fn check_duplicate_transaction_inputs(tx: &Transaction) -> TxResult<()> {
    let mut existing = HashSet::new();
    for input in &tx.inputs {
        if !existing.insert(input.previous_outpoint) {
            return Err(TxRuleError::TxDuplicateInputs);
        }
    }
    Ok(())
}

fn check_gas(tx: &Transaction) -> TxResult<()> {
    // This should be revised if subnetworks are activated (along with other validations that weren't copied from kaspad)
    if tx.gas > 0 {
        return Err(TxRuleError::TxHasGas);
    }
    Ok(())
}

fn check_transaction_payload(tx: &Transaction) -> TxResult<()> {
    // This should be revised if subnetworks are activated (along with other validations that weren't copied from kaspad)
    if !tx.is_coinbase() && !tx.payload.is_empty() {
        return Err(TxRuleError::TxHasGas);
    }
    Ok(())
}

fn check_transaction_version(tx: &Transaction) -> TxResult<()> {
    if tx.version != TX_VERSION {
        return Err(TxRuleError::UnknownTxVersion(tx.version));
    }
    Ok(())
}
