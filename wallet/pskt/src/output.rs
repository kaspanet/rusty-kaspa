use crate::pskt::KeySource;
use crate::utils::combine_if_no_conflicts;
use derive_builder::Builder;
use kaspa_consensus_core::tx::ScriptPublicKey;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, ops::Add};

#[derive(Builder, Default, Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[builder(default)]
pub struct Output {
    /// The output's amount (serialized as sompi).
    pub amount: u64,
    /// The script for this output, also known as the scriptPubKey.
    pub script_public_key: ScriptPublicKey,
    #[builder(setter(strip_option))]
    #[serde(with = "kaspa_utils::serde_bytes_optional")]
    /// The redeem script for this output.
    pub redeem_script: Option<Vec<u8>>,
    /// A map from public keys needed to spend this output to their
    /// corresponding master key fingerprints and derivation paths.
    pub bip32_derivations: BTreeMap<secp256k1::PublicKey, Option<KeySource>>,
    /// Proprietary key-value pairs for this output.
    pub proprietaries: BTreeMap<String, serde_value::Value>,
    #[serde(flatten)]
    /// Unknown key-value pairs for this output.
    pub unknowns: BTreeMap<String, serde_value::Value>,
}

impl Add for Output {
    type Output = Result<Self, CombineError>;

    fn add(mut self, rhs: Self) -> Self::Output {
        if self.amount != rhs.amount {
            return Err(CombineError::AmountMismatch { this: self.amount, that: rhs.amount });
        }
        if self.script_public_key != rhs.script_public_key {
            return Err(CombineError::ScriptPubkeyMismatch { this: self.script_public_key, that: rhs.script_public_key });
        }
        self.redeem_script = match (self.redeem_script.take(), rhs.redeem_script) {
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

/// Error combining two output maps.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error("The amounts are not the same")]
    AmountMismatch {
        /// Attempted to combine a PSKT with `this` previous txid.
        this: u64,
        /// Into a PSKT with `that` previous txid.
        that: u64,
    },
    #[error("The script_pubkeys are not the same")]
    ScriptPubkeyMismatch {
        /// Attempted to combine a PSKT with `this` script_pubkey.
        this: ScriptPublicKey,
        /// Into a PSKT with `that` script_pubkey.
        that: ScriptPublicKey,
    },
    #[error("Two different redeem scripts detected")]
    NotCompatibleRedeemScripts { this: Vec<u8>, that: Vec<u8> },

    #[error("Two different derivations for the same key")]
    NotCompatibleBip32Derivations(#[from] crate::utils::Error<secp256k1::PublicKey, Option<KeySource>>),
    #[error("Two different unknown field values")]
    NotCompatibleUnknownField(crate::utils::Error<String, serde_value::Value>),
    #[error("Two different proprietary values")]
    NotCompatibleProprietary(crate::utils::Error<String, serde_value::Value>),
}
