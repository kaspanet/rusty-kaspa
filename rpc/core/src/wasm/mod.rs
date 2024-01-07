use crate::model::*;
use kaspa_consensus_core::tx;
use kaspa_consensus_wasm::*;
use std::sync::Arc;

impl From<RpcUtxosByAddressesEntry> for UtxoEntry {
    fn from(entry: RpcUtxosByAddressesEntry) -> UtxoEntry {
        UtxoEntry { address: entry.address, outpoint: entry.outpoint.into(), entry: entry.utxo_entry }
    }
}

impl From<RpcUtxosByAddressesEntry> for UtxoEntryReference {
    fn from(entry: RpcUtxosByAddressesEntry) -> Self {
        Self { utxo: Arc::new(entry.into()) }
    }
}

impl From<TransactionInput> for RpcTransactionInput {
    fn from(tx_input: TransactionInput) -> Self {
        let inner = tx_input.inner();
        RpcTransactionInput {
            previous_outpoint: inner.previous_outpoint.clone().into(),
            signature_script: inner.signature_script.clone(),
            sequence: inner.sequence,
            sig_op_count: inner.sig_op_count,
            verbose_data: None,
        }
    }
}

impl From<TransactionOutput> for RpcTransactionOutput {
    fn from(output: TransactionOutput) -> Self {
        let inner = output.inner();
        RpcTransactionOutput { value: inner.value, script_public_key: inner.script_public_key.clone(), verbose_data: None }
    }
}

impl From<Transaction> for RpcTransaction {
    fn from(tx: Transaction) -> Self {
        RpcTransaction::from(&tx)
    }
}

impl From<&Transaction> for RpcTransaction {
    fn from(tx: &Transaction) -> Self {
        let inner = tx.inner();
        let inputs: Vec<RpcTransactionInput> =
            inner.inputs.clone().into_iter().map(|input| input.into()).collect::<Vec<RpcTransactionInput>>();
        let outputs: Vec<RpcTransactionOutput> =
            inner.outputs.clone().into_iter().map(|output| output.into()).collect::<Vec<RpcTransactionOutput>>();

        RpcTransaction {
            version: inner.version,
            inputs,
            outputs,
            lock_time: inner.lock_time,
            subnetwork_id: inner.subnetwork_id.clone(),
            gas: inner.gas,
            payload: inner.payload.clone(),
            mass: 0, // TODO: apply mass to all external APIs including wasm
            verbose_data: None,
        }
    }
}

impl From<SignableTransaction> for RpcTransaction {
    fn from(mtx: SignableTransaction) -> Self {
        let tx = tx::SignableTransaction::from(mtx).tx;

        RpcTransaction {
            version: tx.version,
            inputs: RpcTransactionInput::from_transaction_inputs(tx.inputs),
            outputs: RpcTransactionOutput::from_transaction_outputs(tx.outputs),
            lock_time: tx.lock_time,
            subnetwork_id: tx.subnetwork_id,
            gas: tx.gas,
            payload: tx.payload,
            mass: 0, // TODO: apply mass to all external APIs including wasm
            verbose_data: None,
        }
    }
}
