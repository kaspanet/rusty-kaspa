//! Conversion of Transaction related types

use crate::{
    RpcError, RpcOptionalTransaction, RpcOptionalTransactionInput, RpcOptionalTransactionOutput, RpcResult, RpcTransaction,
    RpcTransactionInput, RpcTransactionOutput,
};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<&Transaction> for RpcTransaction {
    fn from(item: &Transaction) -> Self {
        Self {
            version: item.version,
            inputs: item.inputs.iter().map(RpcTransactionInput::from).collect(),
            outputs: item.outputs.iter().map(RpcTransactionOutput::from).collect(),
            lock_time: item.lock_time,
            subnetwork_id: item.subnetwork_id.clone(),
            gas: item.gas,
            payload: item.payload.clone(),
            mass: item.mass(),
            verbose_data: None,
        }
    }
}

impl From<&TransactionOutput> for RpcTransactionOutput {
    fn from(item: &TransactionOutput) -> Self {
        Self { value: item.value, script_public_key: item.script_public_key.clone(), verbose_data: None }
    }
}

impl From<&TransactionInput> for RpcTransactionInput {
    fn from(item: &TransactionInput) -> Self {
        Self {
            previous_outpoint: item.previous_outpoint.into(),
            signature_script: item.signature_script.clone(),
            sequence: item.sequence,
            sig_op_count: item.sig_op_count,
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcTransaction> for Transaction {
    type Error = RpcError;
    fn try_from(item: RpcTransaction) -> RpcResult<Self> {
        let transaction = Transaction::new(
            item.version,
            item.inputs
                .into_iter()
                .map(kaspa_consensus_core::tx::TransactionInput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionInput>>>()?,
            item.outputs
                .into_iter()
                .map(kaspa_consensus_core::tx::TransactionOutput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionOutput>>>()?,
            item.lock_time,
            item.subnetwork_id.clone(),
            item.gas,
            item.payload.clone(),
        );
        transaction.set_mass(item.mass);
        Ok(transaction)
    }
}

impl TryFrom<RpcTransactionOutput> for TransactionOutput {
    type Error = RpcError;
    fn try_from(item: RpcTransactionOutput) -> RpcResult<Self> {
        Ok(Self::new(item.value, item.script_public_key))
    }
}

impl TryFrom<RpcTransactionInput> for TransactionInput {
    type Error = RpcError;
    fn try_from(item: RpcTransactionInput) -> RpcResult<Self> {
        Ok(Self::new(item.previous_outpoint.into(), item.signature_script, item.sequence, item.sig_op_count))
    }
}

// ----------------------------------------------------------------------------
// consensus_core to optional rpc_core
// ----------------------------------------------------------------------------

impl From<&Transaction> for RpcOptionalTransaction {
    fn from(item: &Transaction) -> Self {
        Self {
            version: Some(item.version),
            inputs: item.inputs.iter().map(RpcOptionalTransactionInput::from).collect(),
            outputs: item.outputs.iter().map(RpcOptionalTransactionOutput::from).collect(),
            lock_time: Some(item.lock_time),
            subnetwork_id: Some(item.subnetwork_id.clone()),
            gas: Some(item.gas),
            payload: Some(item.payload.clone()),
            mass: Some(item.mass()),
            verbose_data: None,
        }
    }
}

impl From<&TransactionOutput> for RpcOptionalTransactionOutput {
    fn from(item: &TransactionOutput) -> Self {
        Self { value: Some(item.value), script_public_key: Some(item.script_public_key.clone()), verbose_data: None }
    }
}

impl From<&TransactionInput> for RpcOptionalTransactionInput {
    fn from(item: &TransactionInput) -> Self {
        Self {
            previous_outpoint: Some(item.previous_outpoint.into()),
            signature_script: Some(item.signature_script.clone()),
            sequence: Some(item.sequence),
            sig_op_count: Some(item.sig_op_count),
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// optional rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<RpcOptionalTransaction> for Transaction {
    type Error = RpcError;
    fn try_from(item: RpcOptionalTransaction) -> RpcResult<Self> {
        let transaction = Transaction::new(
            item.version.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "version".to_owned()))?,
            item.inputs
                .into_iter()
                .map(kaspa_consensus_core::tx::TransactionInput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionInput>>>()?,
            item.outputs
                .into_iter()
                .map(kaspa_consensus_core::tx::TransactionOutput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionOutput>>>()?,
            item.lock_time.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "lock_time".to_owned()))?,
            item.subnetwork_id.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "subnetwork_id".to_owned()))?,
            item.gas.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "gas".to_owned()))?,
            item.payload.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "payload".to_owned()))?,
        );
        transaction.set_mass(item.mass.ok_or(RpcError::MissingRpcFieldError("RpcTransaction".to_owned(), "mass".to_owned()))?);
        Ok(transaction)
    }
}

impl TryFrom<RpcOptionalTransactionOutput> for TransactionOutput {
    type Error = RpcError;
    fn try_from(item: RpcOptionalTransactionOutput) -> RpcResult<Self> {
        Ok(Self::new(
            item.value.ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutput".to_owned(), "value".to_owned()))?,
            item.script_public_key
                .ok_or(RpcError::MissingRpcFieldError("RpcTransactionOutput".to_owned(), "script_public_key".to_owned()))?,
        ))
    }
}

impl TryFrom<RpcOptionalTransactionInput> for TransactionInput {
    type Error = RpcError;
    fn try_from(item: RpcOptionalTransactionInput) -> RpcResult<Self> {
        Ok(Self::new(
            item.previous_outpoint
                .ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "previous_outpoint".to_owned()))?
                .try_into()?,
            item.signature_script
                .ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "signature_script".to_owned()))?,
            item.sequence.ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "sequence".to_owned()))?,
            item.sig_op_count.ok_or(RpcError::MissingRpcFieldError("RpcTransactionInput".to_owned(), "sig_op_count".to_owned()))?,
        ))
    }
}
