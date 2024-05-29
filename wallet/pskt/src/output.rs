use crate::KeySource;
use kaspa_consensus_core::tx::ScriptPublicKey;
use std::{collections::BTreeMap, ops::Add};
use derive_builder::Builder;

#[derive(Builder, Default)]
#[builder(setter)]
pub struct Output {
    /// The output's amount (serialized as sompi).
    pub amount: u64,
    /// The script for this output, also known as the scriptPubKey.
    pub script_public_key: ScriptPublicKey,
    #[builder(setter(strip_option))]
    /// The redeem script for this output.
    pub redeem_script: Option<Vec<u8>>,
    /// A map from public keys needed to spend this output to their
    /// corresponding master key fingerprints and derivation paths.
    pub bip32_derivations: BTreeMap<secp256k1::PublicKey, KeySource>,
    //     /// Proprietary key-value pairs for this output. // todo
    //     pub proprietaries: BTreeMap<String, Vec<u8>>,
    //     /// Unknown key-value pairs for this output.
    //     pub unknowns: BTreeMap<String, Vec<u8>>,
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
        self.bip32_derivations.extend(rhs.bip32_derivations);

        Ok(self)
    }
}

/// Error combining two output maps.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error("The amounts are not the same")]
    AmountMismatch {
        /// Attempted to combine a PBST with `this` previous txid.
        this: u64,
        /// Into a PBST with `that` previous txid.
        that: u64,
    },
    #[error("The script_pubkeys are not the same")]
    ScriptPubkeyMismatch {
        /// Attempted to combine a PBST with `this` script_pubkey.
        this: ScriptPublicKey,
        /// Into a PBST with `that` script_pubkey.
        that: ScriptPublicKey,
    },
    #[error("Two different redeem scripts detected")]
    NotCompatibleRedeemScripts { this: Vec<u8>, that: Vec<u8> },
}
