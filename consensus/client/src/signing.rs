//
// This module is currently disabled, kept for potential future re-integration.
//

use crate::imports::*;
use crate::result::Result;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
use kaspa_consensus_core::hashing::*;
use kaspa_consensus_core::hashing::sighash_type::{SigHashType, SIG_HASH_ALL};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{TransactionOutpoint, TransactionOutput, VerifiableTransaction};
// use kaspa_hashes::{Hash, Hasher, HasherBase, TransactionSigningHash};
use crate::transaction::{Transaction,ITransaction};
use crate::input::{ITransactionInput, TransactionInput};
use crate::utxo::{IUtxoEntry,UtxoEntryReference};
use kaspa_hashes::{Hash, Hasher, HasherBase, TransactionSigningHash, TransactionSigningHashECDSA, ZERO_HASH};
use kaspa_consensus_core::hashing::HasherExtensions;
use kaspa_consensus_core::hashing::sighash::*;

#[derive(Default)]
#[wasm_bindgen]
pub struct SigHashCache {
    #[wasm_bindgen(js_name = "previousOutputsHash")]
    pub previous_outputs_hash: Option<Hash>,
    #[wasm_bindgen(js_name = "sequencesHash")]
    pub sequences_hash: Option<Hash>,
    #[wasm_bindgen(js_name = "sigOpCountsHash")]
    pub sig_op_counts_hash: Option<Hash>,
    #[wasm_bindgen(js_name = "outputsHash")]
    pub outputs_hash: Option<Hash>,
}

#[wasm_bindgen]
impl SigHashCache {
    #[wasm_bindgen(constructor)]
    pub fn ctor() -> SigHashCache {
        // let tx = Transaction::try_cast_from(tx)?.as_ref();
        Self::default()
    }

    pub fn previous_outputs_hash(&mut self, tx: &Transaction, hash_type: SigHashType) -> Hash {
        if hash_type.is_sighash_anyone_can_pay() {
            return ZERO_HASH;
        }

        if let Some(previous_outputs_hash) = self.previous_outputs_hash {
            previous_outputs_hash
        } else {
            let mut hasher = TransactionSigningHash::new();
            for input in tx.inner().inputs.iter() {
                hasher.update(input.previous_outpoint.transaction_id.as_bytes());
                hasher.write_u32(input.previous_outpoint.index);
            }
            let previous_outputs_hash = hasher.finalize();
            self.previous_outputs_hash = Some(previous_outputs_hash);
            previous_outputs_hash
        }
    }

    pub fn sequences_hash(&mut self, tx: &Transaction, hash_type: SigHashType) -> Hash {
        if hash_type.is_sighash_single() || hash_type.is_sighash_anyone_can_pay() || hash_type.is_sighash_none() {
            return ZERO_HASH;
        }

        if let Some(sequences_hash) = self.sequences_hash {
            sequences_hash
        } else {
            let mut hasher = TransactionSigningHash::new();
            for input in tx.inner().inputs.iter() {
                hasher.write_u64(input.sequence);
            }
            let sequence_hash = hasher.finalize();
            self.sequences_hash = Some(sequence_hash);
            sequence_hash
        }
    }

    pub fn sig_op_counts_hash(&mut self, tx: &Transaction, hash_type: SigHashType, reused_values: &mut SigHashReusedValues) -> Hash {
        if hash_type.is_sighash_anyone_can_pay() {
            return ZERO_HASH;
        }

        if let Some(sig_op_counts_hash) = self.sig_op_counts_hash {
            sig_op_counts_hash
        } else {
            let mut hasher = TransactionSigningHash::new();
            for input in tx.inputs.iter() {
                hasher.write_u8(input.sig_op_count);
            }
            let sig_op_counts_hash = hasher.finalize();
            self.sig_op_counts_hash = Some(sig_op_counts_hash);
            sig_op_counts_hash
        }
    }

    // pub fn payload_hash(tx: &Transaction) -> Hash {
    //     if tx.subnetwork_id == SUBNETWORK_ID_NATIVE {
    //         return ZERO_HASH;
    //     }

    //     // TODO: Right now this branch will never be executed, since payload is disabled
    //     // for all non coinbase transactions. Once payload is enabled, the payload hash
    //     // should be cached to make it cost O(1) instead of O(tx.inputs.len()).
    //     let mut hasher = TransactionSigningHash::new();
    //     hasher.write_var_bytes(&tx.payload);
    //     hasher.finalize()
    // }

    pub fn outputs_hash(&mut self, tx: &Transaction, hash_type: SigHashType, input_index: usize) -> Hash {
        if hash_type.is_sighash_none() {
            return ZERO_HASH;
        }

        if hash_type.is_sighash_single() {
            // If the relevant output exists - return its hash, otherwise return zero-hash
            if input_index >= tx.outputs.len() {
                return ZERO_HASH;
            }

            let mut hasher = TransactionSigningHash::new();
            hash_output(&mut hasher, &tx.outputs[input_index]);
            return hasher.finalize();
        }

        // Otherwise, return hash of all outputs. Re-use hash if available.
        if let Some(outputs_hash) = reused_values.outputs_hash {
            outputs_hash
        } else {
            let mut hasher = TransactionSigningHash::new();
            for output in tx.outputs.iter() {
                hash_output(&mut hasher, output);
            }
            let outputs_hash = hasher.finalize();
            reused_values.outputs_hash = Some(outputs_hash);
            outputs_hash
        }
    }

    pub fn hash_outpoint(hasher: &mut impl Hasher, outpoint: TransactionOutpoint) {
        hasher.update(outpoint.transaction_id);
        hasher.write_u32(outpoint.index);
    }

    pub fn hash_output(hasher: &mut impl Hasher, output: &TransactionOutput) {
        hasher.write_u64(output.value);
        hash_script_public_key(hasher, &output.script_public_key);
    }

    pub fn hash_script_public_key(hasher: &mut impl Hasher, script_public_key: &ScriptPublicKey) {
        hasher.write_u16(script_public_key.version());
        hasher.write_var_bytes(script_public_key.script());
    }
}

pub fn calc_schnorr_signature_hash(
    tx : ITransaction,
    input : ITransactionInput,
    // utxo : IUtxoEntry,
//    verifiable_tx: &impl VerifiableTransaction,
    input_index: usize,
    // hash_type: SigHashType,
    // reused_values: &mut SigHashReusedValues,
) -> Result<Hash> {
    // let tx = Transaction::try_cast_from(tx.as_ref())?;
    let tx = Transaction::try_cast_from(tx)?;

    let input = TransactionInput::try_cast_from(input)?;
    // let input = TransactionInput::try_cast_from(input.as_ref())?;

    // let utxo = input.

    let utxo = input.as_ref().utxo().ok_or(Error::MissingUtxoEntry)?;

    // let utxo = UtxoEntryReference::try_cast_from(utxo.as_ref())?;

    let tx = cctx::Transaction::from(tx.as_ref());
    let input = cctx::TransactionInput::from(input.as_ref());
    let utxo = cctx::UtxoEntry::from(utxo.as_ref());

    let hash_type = SIG_HASH_ALL;
    let mut reused_values = SigHashReusedValues::new();

    // let input = verifiable_tx.populated_input(input_index);
    // let tx = verifiable_tx.tx();
    let mut hasher = TransactionSigningHash::new();
    hasher
        .write_u16(tx.version)
        .update(previous_outputs_hash(&tx, hash_type, &mut reused_values))
        .update(sequences_hash(&tx, hash_type, &mut reused_values))
        .update(sig_op_counts_hash(&tx, hash_type, &mut reused_values));
    hash_outpoint(&mut hasher, input.previous_outpoint);
    hash_script_public_key(&mut hasher, &utxo.script_public_key);
    hasher
        .write_u64(utxo.amount)
        .write_u64(input.sequence)
        .write_u8(input.sig_op_count)
        .update(outputs_hash(&tx, hash_type, &mut reused_values, input_index))
        .write_u64(tx.lock_time)
        .update(&tx.subnetwork_id)
        .write_u64(tx.gas)
        .update(payload_hash(&tx))
        .write_u8(hash_type.to_u8());
    Ok(hasher.finalize())
}

pub fn calc_ecdsa_signature_hash(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    // reused_values: &mut SigHashReusedValues,
) -> Result<Hash> {
    let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, reused_values)?;
    let mut hasher = TransactionSigningHashECDSA::new();
    hasher.update(hash);
    hasher.finalize()
}
