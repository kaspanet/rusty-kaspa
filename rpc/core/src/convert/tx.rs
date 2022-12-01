use crate::{
    RpcError, RpcResult, RpcScriptPublicKey, RpcScriptVec, RpcTransaction, RpcTransactionInput, RpcTransactionOutput, RpcUtxoEntry,
};
use consensus_core::tx::{ScriptPublicKey, ScriptVec, Transaction, TransactionInput, TransactionOutput, UtxoEntry};

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
            payload: (&item.payload).into(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&TransactionOutput> for RpcTransactionOutput {
    fn from(item: &TransactionOutput) -> Self {
        Self {
            value: item.value,
            script_public_key: (&item.script_public_key).into(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&TransactionInput> for RpcTransactionInput {
    fn from(item: &TransactionInput) -> Self {
        Self {
            previous_outpoint: item.previous_outpoint,
            signature_script: (&item.signature_script).into(),
            sequence: item.sequence,
            sig_op_count: item.sig_op_count,
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&UtxoEntry> for RpcUtxoEntry {
    fn from(item: &UtxoEntry) -> Self {
        Self {
            amount: item.amount,
            script_public_key: (&item.script_public_key).into(),
            block_daa_score: item.block_daa_score,
            is_coinbase: item.is_coinbase,
        }
    }
}

impl From<&ScriptPublicKey> for RpcScriptPublicKey {
    fn from(item: &ScriptPublicKey) -> Self {
        Self { version: item.version(), script_public_key: item.script().into() }
    }
}

impl From<&ScriptVec> for RpcScriptVec {
    fn from(item: &ScriptVec) -> Self {
        (&item.clone().into_vec()).into()
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&RpcTransaction> for Transaction {
    type Error = RpcError;
    fn try_from(item: &RpcTransaction) -> RpcResult<Self> {
        Ok(Transaction::new(
            item.version,
            vec![],
            vec![],
            item.lock_time,
            item.subnetwork_id.clone(),
            item.gas,
            item.payload.as_ref().clone(),
        ))
    }
}

impl TryFrom<&RpcTransactionOutput> for TransactionOutput {
    type Error = RpcError;
    fn try_from(item: &RpcTransactionOutput) -> RpcResult<Self> {
        Ok(Self::new(item.value, (&item.script_public_key).try_into()?))
    }
}

impl TryFrom<&RpcTransactionInput> for TransactionInput {
    type Error = RpcError;
    fn try_from(item: &RpcTransactionInput) -> RpcResult<Self> {
        Ok(Self::new(item.previous_outpoint, item.signature_script.as_ref().clone(), item.sequence, item.sig_op_count))
    }
}

impl TryFrom<&RpcUtxoEntry> for UtxoEntry {
    type Error = RpcError;
    fn try_from(item: &RpcUtxoEntry) -> RpcResult<Self> {
        Ok(Self::new(item.amount, (&item.script_public_key).try_into()?, item.block_daa_score, item.is_coinbase))
    }
}

impl TryFrom<&RpcScriptPublicKey> for ScriptPublicKey {
    type Error = RpcError;
    fn try_from(item: &RpcScriptPublicKey) -> RpcResult<Self> {
        Ok(Self::new(item.version, (&item.script_public_key).try_into()?))
    }
}

impl TryFrom<&RpcScriptVec> for ScriptVec {
    type Error = RpcError;
    fn try_from(item: &RpcScriptVec) -> RpcResult<Self> {
        Ok(ScriptVec::from_slice(item.as_ref().as_slice()))
    }
}
