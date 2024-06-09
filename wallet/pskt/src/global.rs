use crate::utils::combine_if_no_conflicts;
use crate::{KeySource, Version};
use derive_builder::Builder;
use kaspa_consensus_core::tx::TransactionId;
use std::collections::{btree_map, BTreeMap};
use std::ops::Add;
use serde::{Deserialize, Serialize};

type Xpub = kaspa_bip32::ExtendedPublicKey<secp256k1::PublicKey>;

#[derive(Debug, Clone, Builder, Serialize, Deserialize)]
#[builder(default)]
pub struct Global {
    /// The version number of this PSKT.
    pub version: Version,
    /// The version number of the transaction being built.
    pub tx_version: u16,
    #[builder(setter(strip_option))]
    /// The transaction locktime to use if no inputs specify a required locktime.
    pub fallback_lock_time: Option<u64>,

    pub inputs_modifiable: bool,
    pub outputs_modifiable: bool,

    /// The number of inputs in this PSKT.
    pub input_count: usize,
    /// The number of outputs in this PSKT.
    pub output_count: usize,
    /// A map from xpub to the used key fingerprint and derivation path as defined by BIP 32.
    pub xpubs: BTreeMap<Xpub, KeySource>,
    pub id: Option<TransactionId>,
    /// Proprietary key-value pairs for this output.
    pub proprietaries: BTreeMap<String, serde_value::Value>,
    /// Unknown key-value pairs for this output.
    pub unknowns: BTreeMap<String, serde_value::Value>,
}

impl Add for Global {
    type Output = Result<Self, CombineError>;

    fn add(mut self, rhs: Self) -> Self::Output {
        if self.version != rhs.version {
            return Err(CombineError::VersionMismatch { this: self.version, that: rhs.version });
        }
        if self.tx_version != rhs.tx_version {
            return Err(CombineError::TxVersionMismatch { this: self.tx_version, that: rhs.tx_version });
        }
        // BIP 174: The Combiner must remove any duplicate key-value pairs, in accordance with
        //          the specification. It can pick arbitrarily when conflicts occur.

        // Merging xpubs
        for (xpub, KeySource { key_fingerprint: fingerprint1, derivation_path: derivation1 }) in rhs.xpubs {
            match self.xpubs.entry(xpub) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(KeySource::new(fingerprint1, derivation1));
                }
                btree_map::Entry::Occupied(mut entry) => {
                    // Here in case of the conflict we select the version with algorithm:
                    // 1) if everything is equal we do nothing
                    // 2) report an error if
                    //    - derivation paths are equal and fingerprints are not
                    //    - derivation paths are of the same length, but not equal
                    //    - derivation paths has different length, but the shorter one
                    //      is not the strict suffix of the longer one
                    // 3) choose longest derivation otherwise

                    let KeySource { key_fingerprint: fingerprint2, derivation_path: derivation2 } = entry.get().clone();

                    if (derivation1 == derivation2 && fingerprint1 == fingerprint2)
                        || (derivation1.len() < derivation2.len()
                            && derivation1.as_ref() == &derivation2.as_ref()[derivation2.len() - derivation1.len()..])
                    {
                        continue;
                    } else if derivation2.as_ref() == &derivation1.as_ref()[derivation1.len() - derivation2.len()..] {
                        entry.insert(KeySource::new(fingerprint1, derivation1));
                        continue;
                    }
                    return Err(CombineError::InconsistentKeySources(entry.key().clone()));
                }
            }
        }

        // todo combine inputs count? combine modifiers?
        // pub inputs_modifiable: bool,
        // pub outputs_modifiable: bool,
        //
        // /// The number of inputs in this PSKT.
        // pub input_count: usize,
        // /// The number of outputs in this PSKT.
        // pub output_count: usize,

        self.proprietaries =
            combine_if_no_conflicts(self.proprietaries, rhs.proprietaries).map_err(CombineError::NotCompatibleProprietary)?;
        self.unknowns = combine_if_no_conflicts(self.unknowns, rhs.unknowns).map_err(CombineError::NotCompatibleUnknownField)?;
        Ok(self)
    }
}

impl Default for Global {
    fn default() -> Self {
        Global {
            version: Version::Zero,
            tx_version: kaspa_consensus_core::constants::TX_VERSION,
            fallback_lock_time: None,
            inputs_modifiable: false,
            outputs_modifiable: false,
            input_count: 0,
            output_count: 0,
            xpubs: Default::default(),
            id: None,
            proprietaries: Default::default(),
            unknowns: Default::default(),
        }
    }
}

/// Error combining two global maps.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error("The version numbers are not the same")]
    /// The version numbers are not the same.
    VersionMismatch {
        /// Attempted to combine a PBST with `this` version.
        this: Version,
        /// Into a PBST with `that` version.
        that: Version,
    },
    #[error("The transaction version numbers are not the same")]
    TxVersionMismatch {
        /// Attempted to combine a PBST with `this` tx version.
        this: u16,
        /// Into a PBST with `that` tx version.
        that: u16,
    },
    #[error("combining PSBT, key-source conflict for xpub {0}")]
    /// Xpubs have inconsistent key sources.
    InconsistentKeySources(Xpub),

    #[error("Two different unknown field values")]
    NotCompatibleUnknownField(crate::utils::Error<String, serde_value::Value>),
    #[error("Two different proprietary values")]
    NotCompatibleProprietary(crate::utils::Error<String, serde_value::Value>),
}