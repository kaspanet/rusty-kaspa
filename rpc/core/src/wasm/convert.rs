use crate::model::*;
use kaspa_consensus_client::*;
use kaspa_consensus_core::tx as cctx;
use std::sync::Arc;

impl From<RpcUtxosByAddressesEntry> for UtxoEntry {
    fn from(entry: RpcUtxosByAddressesEntry) -> UtxoEntry {
        let RpcUtxosByAddressesEntry { address, outpoint, utxo_entry } = entry;
        let cctx::UtxoEntry { amount, script_public_key, block_daa_score, is_coinbase } = utxo_entry;
        UtxoEntry { address, outpoint: outpoint.into(), amount, script_public_key, block_daa_score, is_coinbase }
    }
}

impl From<RpcUtxosByAddressesEntry> for UtxoEntryReference {
    fn from(entry: RpcUtxosByAddressesEntry) -> Self {
        Self { utxo: Arc::new(entry.into()) }
    }
}

impl From<&RpcUtxosByAddressesEntry> for UtxoEntryReference {
    fn from(entry: &RpcUtxosByAddressesEntry) -> Self {
        Self { utxo: Arc::new(entry.clone().into()) }
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {
        use kaspa_consensus_core::tx;
        use kaspa_consensus_wasm::*;

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

        impl TryFrom<SignableTransaction> for RpcTransaction {
            type Error = crate::error::RpcError;
            fn try_from(mtx: SignableTransaction) -> crate::error::RpcResult<Self> {
                let tx = tx::SignableTransaction::from(mtx).tx;

                Ok(RpcTransaction {
                    version: tx.version,
                    inputs: RpcTransactionInput::from_transaction_inputs(tx.inputs),
                    outputs: RpcTransactionOutput::from_transaction_outputs(tx.outputs),
                    lock_time: tx.lock_time,
                    subnetwork_id: tx.subnetwork_id,
                    gas: tx.gas,
                    payload: tx.payload,
                    mass: 0, // TODO: apply mass to all external APIs including wasm
                    verbose_data: None,
                })
            }
        }
    }
}
