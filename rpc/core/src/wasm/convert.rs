//!
//! WASM specific conversion functions
//!

use crate::{model::*, RpcError, RpcResult};
use kaspa_consensus_client::*;
use std::sync::Arc;

impl TryFrom<RpcUtxosByAddressesEntry> for UtxoEntry {
    type Error = RpcError;

    fn try_from(entry: RpcUtxosByAddressesEntry) -> RpcResult<UtxoEntry> {
        let RpcUtxosByAddressesEntry { address, outpoint, utxo_entry } = entry;
        let RpcUtxoEntry { amount, script_public_key, block_daa_score, is_coinbase, .. } = utxo_entry;
        Ok(UtxoEntry {
            address,
            outpoint: outpoint.try_into()?,
            amount: amount.ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "amount".to_string()))?,
            script_public_key: script_public_key
                .ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "script_public_key".to_string()))?,
            block_daa_score: block_daa_score
                .ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "block_daa_score".to_string()))?,
            is_coinbase: is_coinbase.ok_or(RpcError::MissingRpcFieldError("RpcUtxoEntry".to_string(), "is_coinbase".to_string()))?,
        })
    }
}

impl TryFrom<RpcUtxosByAddressesEntry> for UtxoEntryReference {
    type Error = RpcError;

    fn try_from(entry: RpcUtxosByAddressesEntry) -> RpcResult<Self> {
        Ok(Self { utxo: Arc::new(entry.try_into()?) })
    }
}

impl TryFrom<&RpcUtxosByAddressesEntry> for UtxoEntryReference {
    type Error = RpcError;

    fn try_from(entry: &RpcUtxosByAddressesEntry) -> RpcResult<Self> {
        Ok(Self { utxo: Arc::new(entry.clone().try_into()?) })
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "wasm32-sdk")] {

        impl From<TransactionInput> for RpcTransactionInput {
            fn from(tx_input: TransactionInput) -> Self {
                let inner = tx_input.inner();
                RpcTransactionInput {
                    previous_outpoint: Some(inner.previous_outpoint.clone().into()),
                    signature_script: Some(inner.signature_script.clone().unwrap_or_default()),
                    sequence: Some(inner.sequence),
                    sig_op_count: Some(inner.sig_op_count),
                    verbose_data: None,
                }
            }
        }

        impl From<TransactionOutput> for RpcTransactionOutput {
            fn from(output: TransactionOutput) -> Self {
                let inner = output.inner();
                RpcTransactionOutput { value: Some(inner.value), script_public_key: Some(inner.script_public_key.clone()), verbose_data: None }
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
                    version: Some(inner.version),
                    inputs,
                    outputs,
                    lock_time: Some(inner.lock_time),
                    subnetwork_id: Some(inner.subnetwork_id.clone()),
                    gas: Some(inner.gas),
                    payload: Some(inner.payload.clone()),
                    mass: Some(inner.mass),
                    verbose_data: None,
                }
            }
        }
    }
}
