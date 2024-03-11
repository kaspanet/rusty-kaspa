use crate::imports::*;
use crate::result::Result;
use kaspa_consensus_core::hashing::sighash::SigHashReusedValues;
use kaspa_consensus_core::hashing::*;
use kaspa_consensus_core::hashing::sighash_type::{SigHashType, SIG_HASH_ALL};
use kaspa_consensus_core::tx::{TransactionOutpoint, TransactionOutput, VerifiableTransaction};
// use kaspa_hashes::{Hash, Hasher, HasherBase, TransactionSigningHash};
use crate::transaction::{Transaction,ITransaction};
use crate::input::{ITransactionInput, TransactionInput};
use crate::utxo::{IUtxoEntry,UtxoEntryReference};
use kaspa_hashes::{Hash, Hasher, HasherBase, TransactionSigningHash, TransactionSigningHashECDSA, ZERO_HASH};
use kaspa_consensus_core::hashing::HasherExtensions;
use kaspa_consensus_core::hashing::sighash::*;

pub fn calc_schnorr_signature_hash(
    tx : ITransaction,
    input : ITransactionInput,
    utxo : IUtxoEntry,
//    verifiable_tx: &impl VerifiableTransaction,
    // input_index: usize,
    // hash_type: SigHashType,
    // reused_values: &mut SigHashReusedValues,
) -> Result<Hash> {
    let tx = Transaction::try_cast_from(tx.as_ref())?;
    let input = TransactionInput::try_cast_from(input.as_ref())?;
    let utxo = UtxoEntryReference::try_cast_from(utxo.as_ref())?;

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
    hasher.finalize()
}

pub fn calc_ecdsa_signature_hash(
    tx: &impl VerifiableTransaction,
    input_index: usize,
    hash_type: SigHashType,
    reused_values: &mut SigHashReusedValues,
) -> Hash {
    let hash = calc_schnorr_signature_hash(tx, input_index, hash_type, reused_values);
    let mut hasher = TransactionSigningHashECDSA::new();
    hasher.update(hash);
    hasher.finalize()
}
