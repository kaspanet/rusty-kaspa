use crate::{ChainCode, ChildNumber, Depth, KeyFingerprint};

/// Extended key attributes: fields common to extended keys including depth,
/// fingerprints, child numbers, and chain codes.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ExtendedKeyAttrs {
    /// Depth in the key derivation hierarchy.
    pub depth: Depth,

    /// Parent fingerprint.
    pub parent_fingerprint: KeyFingerprint,

    /// Child number.
    pub child_number: ChildNumber,

    /// Chain code.
    pub chain_code: ChainCode,
}
