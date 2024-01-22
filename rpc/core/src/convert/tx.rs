use crate::{
    RpcError, RpcResult, RpcTransaction, RpcTransactionAcceptanceData, RpcTransactionInclusionData, RpcTransactionInput,
    RpcTransactionOutput,
};
use kaspa_consensus_core::{
    header::CompactHeaderData,
    tx::{Transaction, TransactionInput, TransactionOutput},
};
use kaspa_index_core::models::txindex::{BlockAcceptanceOffset, TxOffset};

// ----------------------------------------------------------------------------
// consensus_core to rpc_core
// ----------------------------------------------------------------------------

impl From<(&BlockAcceptanceOffset, &CompactHeaderData)> for RpcTransactionAcceptanceData {
    fn from(item: (&BlockAcceptanceOffset, &CompactHeaderData)) -> Self {
        Self {
            accepting_block_hash: item.0.accepting_block,
            accepting_block_mergeset_index: item.0.mergeset_index,
            accepting_block_blue_score: item.1.blue_score,
            accepting_block_time: item.1.timestamp,
            accepting_block_daa_score: item.1.daa_score,
        }
    }
}

impl From<(&TxOffset, &CompactHeaderData)> for RpcTransactionInclusionData {
    fn from(item: (&TxOffset, &CompactHeaderData)) -> Self {
        Self {
            including_block_hash: item.0.including_block,
            including_block_transaction_index: item.0.transaction_index,
            including_block_daa_score: item.1.daa_score,
            including_block_time: item.1.timestamp,
        }
    }
}

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
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&TransactionOutput> for RpcTransactionOutput {
    fn from(item: &TransactionOutput) -> Self {
        Self {
            value: item.value,
            script_public_key: item.script_public_key.clone(),
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

impl From<&TransactionInput> for RpcTransactionInput {
    fn from(item: &TransactionInput) -> Self {
        Self {
            previous_outpoint: item.previous_outpoint,
            signature_script: item.signature_script.clone(),
            sequence: item.sequence,
            sig_op_count: item.sig_op_count,
            // TODO: Implement a populating process inspired from kaspad\app\rpc\rpccontext\verbosedata.go
            verbose_data: None,
        }
    }
}

// ----------------------------------------------------------------------------
// rpc_core to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&RpcTransaction> for Transaction {
    type Error = RpcError;
    fn try_from(item: &RpcTransaction) -> RpcResult<Self> {
        let transaction = Transaction::new(
            item.version,
            item.inputs
                .iter()
                .map(kaspa_consensus_core::tx::TransactionInput::try_from)
                .collect::<RpcResult<Vec<kaspa_consensus_core::tx::TransactionInput>>>()?,
            item.outputs
                .iter()
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

impl TryFrom<&RpcTransactionOutput> for TransactionOutput {
    type Error = RpcError;
    fn try_from(item: &RpcTransactionOutput) -> RpcResult<Self> {
        Ok(Self::new(item.value, item.script_public_key.clone()))
    }
}

impl TryFrom<&RpcTransactionInput> for TransactionInput {
    type Error = RpcError;
    fn try_from(item: &RpcTransactionInput) -> RpcResult<Self> {
        Ok(Self::new(item.previous_outpoint, item.signature_script.clone(), item.sequence, item.sig_op_count))
    }
}
