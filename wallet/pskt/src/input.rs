// todo add builder, separate signatures

use crate::{KeySource, PartialSigs};
use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
use kaspa_consensus_core::{
    hashing::sighash_type::SigHashType,
    tx::{TransactionId, TransactionOutpoint, UtxoEntry},
};
use std::collections::BTreeMap;
use std::ops::Add;

// todo add unknown field? combine them by deduplicating, if there are different values - return error?
pub struct Input {
    pub utxo_entry: Option<UtxoEntry>,
    pub previous_outpoint: TransactionOutpoint,
    /// The sequence number of this input.
    ///
    /// If omitted, assumed to be the final sequence number
    pub sequence: Option<u64>,
    /// The minimum Unix timestamp that this input requires to be set as the transaction's lock time.
    pub min_time: Option<u64>,
    /// A map from public keys to their corresponding signature as would be
    /// pushed to the stack from a scriptSig.
    pub partial_sigs: PartialSigs,
    /// The sighash type to be used for this input. Signatures for this input
    /// must use the sighash type.
    pub sighash_type: SigHashType,
    /// The redeem script for this input.
    pub redeem_script: Option<Vec<u8>>,
    pub sig_op_count: Option<u8>,
    /// A map from public keys needed to sign this input to their corresponding
    /// master key fingerprints and derivation paths.
    pub bip32_derivations: BTreeMap<secp256k1::PublicKey, KeySource>,
    /// The finalized, fully-constructed scriptSig with signatures and any other
    /// scripts necessary for this input to pass validation.
    pub final_script_sig: Option<Vec<u8>>,
}

impl Default for Input {
    fn default() -> Self {
        Self {
            utxo_entry: Default::default(),
            previous_outpoint: Default::default(),
            sequence: Default::default(),
            min_time: Default::default(),
            partial_sigs: Default::default(),
            sighash_type: SIG_HASH_ALL,
            redeem_script: Default::default(),
            sig_op_count: Default::default(),
            bip32_derivations: Default::default(),
            final_script_sig: Default::default(),
        }
    }
}

impl Add for Input {
    type Output = Result<Self, CombineError>;

    fn add(mut self, rhs: Self) -> Self::Output {
        if self.previous_outpoint.transaction_id != rhs.previous_outpoint.transaction_id {
            return Err(CombineError::PreviousTxidMismatch {
                this: self.previous_outpoint.transaction_id,
                that: rhs.previous_outpoint.transaction_id,
            });
        }

        if self.previous_outpoint.index != rhs.previous_outpoint.index {
            return Err(CombineError::SpentOutputIndexMismatch {
                this: self.previous_outpoint.index,
                that: rhs.previous_outpoint.index,
            });
        }
        // todo check if it's correct
        self.utxo_entry = match (self.utxo_entry.take(), rhs.utxo_entry) {
            (None, None) => None,
            (Some(utxo), None) | (None, Some(utxo)) => Some(utxo),
            (Some(left), Some(right)) if left == right => Some(left),
            (Some(left), Some(right)) => return Err(CombineError::NotCompatibleUtxos { this: left, that: right }),
        };

        // todo discuss merging. if sequence is equal - combine, otherwise use input which has bigger sequence number as is
        self.sequence = self.sequence.max(rhs.sequence);
        self.min_time = self.min_time.max(rhs.min_time);
        self.partial_sigs.extend(rhs.partial_sigs);
        // todo combine sighash? or always use sighash all since all signatures must be passed after competition of construction step
        // self.sighash_type

        self.redeem_script = match (self.redeem_script.take(), rhs.redeem_script) {
            (None, None) => None,
            (Some(script), None) | (None, Some(script)) => Some(script),
            (Some(script_left), Some(script_right)) if script_left == script_right => Some(script_left),
            (Some(script_left), Some(script_right)) => {
                return Err(CombineError::NotCompatibleRedeemScripts { this: script_left, that: script_right })
            }
        };
        self.bip32_derivations.extend(rhs.bip32_derivations);

        // todo Does Combiner allowed to change final script sig??
        // self.final_script_sig = match (self.final_script_sig.take(), rhs.final_script_sig) {
        //     (None, None) => None,
        //     (Some(script), None) | (None, Some(script)) => Some(script),
        //     (Some(script_left), Some(script_right)) if script_left == script_right => Some(script_left),
        //     (Some(script_left), Some(script_right)) => {
        //         return Err(CombineError::NotCompatibleRedeemScripts { this: script_left, that: script_right })
        //     }
        // };
        Ok(self)
    }
}

/// Error combining two input maps.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error("The previous txids are not the same")]
    PreviousTxidMismatch {
        /// Attempted to combine a PBST with `this` previous txid.
        this: TransactionId,
        /// Into a PBST with `that` previous txid.
        that: TransactionId,
    },
    #[error("The spent output indexes are not the same")]
    SpentOutputIndexMismatch {
        /// Attempted to combine a PBST with `this` spent output index.
        this: u32,
        /// Into a PBST with `that` spent output index.
        that: u32,
    },
    #[error("Two different redeem scripts detected")]
    NotCompatibleRedeemScripts { this: Vec<u8>, that: Vec<u8> },
    #[error("Two different utxos detected")]
    NotCompatibleUtxos { this: UtxoEntry, that: UtxoEntry },
}
