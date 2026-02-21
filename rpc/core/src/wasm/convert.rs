//!
//! WASM specific conversion functions
//!

use crate::model::*;
use kaspa_consensus_client::*;
use std::sync::Arc;

impl From<RpcUtxosByAddressesEntry> for UtxoEntry {
    fn from(entry: RpcUtxosByAddressesEntry) -> UtxoEntry {
        let RpcUtxosByAddressesEntry { address, outpoint, utxo_entry } = entry;
        let RpcUtxoEntry { amount, script_public_key, block_daa_score, is_coinbase } = utxo_entry;
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

        impl From<RpcOptionalHeader> for OptionalHeader {
            fn from(header: RpcOptionalHeader) -> Self {
                OptionalHeader::new_from_fields(
                    header.hash,
                    header.version,
                    header.parents_by_level.map(CompressedParents::from),
                    header.hash_merkle_root,
                    header.accepted_id_merkle_root,
                    header.utxo_commitment,
                    header.timestamp,
                    header.bits,
                    header.nonce,
                    header.daa_score,
                    header.blue_work,
                    header.blue_score,
                    header.pruning_point,
                )
            }
        }

        impl From<&RpcOptionalHeader> for OptionalHeader {
            fn from(header: &RpcOptionalHeader) -> Self {
                OptionalHeader::new_from_fields(
                    header.hash,
                    header.version,
                    header.parents_by_level.clone().map(CompressedParents::from),
                    header.hash_merkle_root,
                    header.accepted_id_merkle_root,
                    header.utxo_commitment,
                    header.timestamp,
                    header.bits,
                    header.nonce,
                    header.daa_score,
                    header.blue_work,
                    header.blue_score,
                    header.pruning_point,
                )
            }
        }

        impl From<TransactionInput> for RpcTransactionInput {
            fn from(tx_input: TransactionInput) -> Self {
                let inner = tx_input.inner();
                RpcTransactionInput {
                    previous_outpoint: inner.previous_outpoint.clone().into(),
                    signature_script: inner.signature_script.clone().unwrap_or_default(),
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
                    mass: inner.mass,
                    verbose_data: None,
                }
            }
        }

        impl From<TransactionInput> for RpcOptionalTransactionInput {
            fn from(tx_input: TransactionInput) -> Self {
                let inner = tx_input.inner();
                RpcOptionalTransactionInput {
                    previous_outpoint: Some(inner.previous_outpoint.clone().into()),
                    signature_script: Some(inner.signature_script.clone().unwrap_or_default()),
                    sequence: Some(inner.sequence),
                    sig_op_count: Some(inner.sig_op_count),
                    verbose_data: None,
                }
            }
        }

        impl From<TransactionOutput> for RpcOptionalTransactionOutput {
            fn from(output: TransactionOutput) -> Self {
                let inner = output.inner();
                RpcOptionalTransactionOutput { value: Some(inner.value), script_public_key: Some(inner.script_public_key.clone()), verbose_data: None }
            }
        }

        impl From<Transaction> for RpcOptionalTransaction {
            fn from(tx: Transaction) -> Self {
                RpcOptionalTransaction::from(&tx)
            }
        }

        impl From<&Transaction> for RpcOptionalTransaction {

            fn from(tx: &Transaction) -> Self {
                let inner = tx.inner();
                let inputs: Vec<RpcOptionalTransactionInput> =
                    inner.inputs.clone().into_iter().map(|input| input.into()).collect::<Vec<RpcOptionalTransactionInput>>();
                    let outputs: Vec<RpcOptionalTransactionOutput> =
                    inner.outputs.clone().into_iter().map(|output| output.into()).collect::<Vec<RpcOptionalTransactionOutput>>();

                RpcOptionalTransaction {
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
