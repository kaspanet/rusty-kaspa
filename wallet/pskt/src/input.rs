use crate::pskt::{KeySource, PartialSigs};
use crate::utils::{combine_if_no_conflicts, Error as CombineMapErr};
use derive_builder::Builder;
use kaspa_consensus_core::{
    hashing::sighash_type::{SigHashType, SIG_HASH_ALL},
    tx::{TransactionId, TransactionOutpoint, UtxoEntry},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, marker::PhantomData, ops::Add};

// todo add unknown field? combine them by deduplicating, if there are different values - return error?
#[derive(Builder, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[builder(default)]
#[builder(setter(skip))]
pub struct Input {
    #[builder(setter(strip_option))]
    pub utxo_entry: Option<UtxoEntry>,
    #[builder(setter)]
    pub previous_outpoint: TransactionOutpoint,
    /// The sequence number of this input.
    ///
    /// If omitted, assumed to be the final sequence number
    pub sequence: Option<u64>,
    #[builder(setter)]
    /// The minimum Unix timestamp that this input requires to be set as the transaction's lock time.
    pub min_time: Option<u64>,
    /// A map from public keys to their corresponding signature as would be
    /// pushed to the stack from a scriptSig.
    pub partial_sigs: PartialSigs,
    #[builder(setter)]
    /// The sighash type to be used for this input. Signatures for this input
    /// must use the sighash type.
    pub sighash_type: SigHashType,
    #[serde(with = "kaspa_utils::serde_bytes_optional")]
    #[builder(setter(strip_option))]
    /// The redeem script for this input.
    pub redeem_script: Option<Vec<u8>>,
    #[builder(setter(strip_option))]
    pub sig_op_count: Option<u8>,
    /// A map from public keys needed to sign this input to their corresponding
    /// master key fingerprints and derivation paths.
    pub bip32_derivations: BTreeMap<secp256k1::PublicKey, Option<KeySource>>,
    #[serde(with = "kaspa_utils::serde_bytes_optional")]
    /// The finalized, fully-constructed scriptSig with signatures and any other
    /// scripts necessary for this input to pass validation.
    pub final_script_sig: Option<Vec<u8>>,
    #[serde(skip_serializing, default)]
    pub(crate) hidden: PhantomData<()>, // prevents manual filling of fields
    #[builder(setter)]
    /// Proprietary key-value pairs for this output.
    pub proprietaries: BTreeMap<String, serde_value::Value>,
    #[serde(flatten)]
    #[builder(setter)]
    /// Unknown key-value pairs for this output.
    pub unknowns: BTreeMap<String, serde_value::Value>,
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
            hidden: Default::default(),
            proprietaries: Default::default(),
            unknowns: Default::default(),
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
        // todo combine sighash? or always use sighash all since all signatures must be passed after completion of construction step
        // self.sighash_type

        self.redeem_script = match (self.redeem_script.take(), rhs.redeem_script) {
            (None, None) => None,
            (Some(script), None) | (None, Some(script)) => Some(script),
            (Some(script_left), Some(script_right)) if script_left == script_right => Some(script_left),
            (Some(script_left), Some(script_right)) => {
                return Err(CombineError::NotCompatibleRedeemScripts { this: script_left, that: script_right })
            }
        };

        // todo Does Combiner allowed to change final script sig??
        self.final_script_sig = match (self.final_script_sig.take(), rhs.final_script_sig) {
            (None, None) => None,
            (Some(script), None) | (None, Some(script)) => Some(script),
            (Some(script_left), Some(script_right)) if script_left == script_right => Some(script_left),
            (Some(script_left), Some(script_right)) => {
                return Err(CombineError::NotCompatibleRedeemScripts { this: script_left, that: script_right })
            }
        };

        self.bip32_derivations = combine_if_no_conflicts(self.bip32_derivations, rhs.bip32_derivations)?;
        self.proprietaries =
            combine_if_no_conflicts(self.proprietaries, rhs.proprietaries).map_err(CombineError::NotCompatibleProprietary)?;
        self.unknowns = combine_if_no_conflicts(self.unknowns, rhs.unknowns).map_err(CombineError::NotCompatibleUnknownField)?;

        Ok(self)
    }
}

/// Error combining two input maps.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error("The previous txids are not the same")]
    PreviousTxidMismatch {
        /// Attempted to combine a PSKT with `this` previous txid.
        this: TransactionId,
        /// Into a PSKT with `that` previous txid.
        that: TransactionId,
    },
    #[error("The spent output indexes are not the same")]
    SpentOutputIndexMismatch {
        /// Attempted to combine a PSKT with `this` spent output index.
        this: u32,
        /// Into a PSKT with `that` spent output index.
        that: u32,
    },
    #[error("Two different redeem scripts detected")]
    NotCompatibleRedeemScripts { this: Vec<u8>, that: Vec<u8> },
    #[error("Two different utxos detected")]
    NotCompatibleUtxos { this: UtxoEntry, that: UtxoEntry },

    #[error("Two different derivations for the same key")]
    NotCompatibleBip32Derivations(#[from] CombineMapErr<secp256k1::PublicKey, Option<KeySource>>),
    #[error("Two different unknown field values")]
    NotCompatibleUnknownField(CombineMapErr<String, serde_value::Value>),
    #[error("Two different proprietary values")]
    NotCompatibleProprietary(CombineMapErr<String, serde_value::Value>),
}
