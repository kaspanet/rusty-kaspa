use crate::{KeySource, Version};
use std::collections::{btree_map, BTreeMap};
use std::ops::Add;

type Xpub = kaspa_bip32::ExtendedPublicKey<secp256k1::PublicKey>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Global {
    /// The version number of this PSKT.
    pub version: Version,
    /// The version number of the transaction being built.
    pub tx_version: u16,
    /// The transaction locktime to use if no inputs specify a required locktime.
    pub fallback_lock_time: Option<u64>,
    pub fallback_sequence: Option<u64>,

    pub inputs_modifiable: bool,
    pub outputs_modifiable: bool,

    /// The number of inputs in this PSKT.
    pub input_count: usize,
    /// The number of outputs in this PSKT.
    pub output_count: usize,
    /// A map from xpub to the used key fingerprint and derivation path as defined by BIP 32.
    pub xpubs: BTreeMap<Xpub, KeySource>,
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
        for (xpub, (fingerprint1, derivation1)) in rhs.xpubs {
            match self.xpubs.entry(xpub) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert((fingerprint1, derivation1));
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

                    let (fingerprint2, derivation2) = entry.get().clone();

                    // todo fix me

                    if (derivation1 == derivation2 && fingerprint1 == fingerprint2)
                        || (derivation1.len() < derivation2.len()
                        && derivation1.as_ref()
                        == &derivation2.as_ref()[derivation2.len() - derivation1.len()..])
                    {
                        continue;
                    } else if derivation2.as_ref()
                        == &derivation1.as_ref()[derivation1.len() - derivation2.len()..]
                    {
                        entry.insert((fingerprint1, derivation1));
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
        Ok(self)
    }
}

impl Default for Global {
    fn default() -> Self {
        Global {
            version: Version::Zero,
            tx_version: kaspa_consensus_core::constants::TX_VERSION,
            fallback_lock_time: None,
            fallback_sequence: None,
            inputs_modifiable: false,
            outputs_modifiable: false,
            input_count: 0,
            output_count: 0,
            xpubs: Default::default(),
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
}
