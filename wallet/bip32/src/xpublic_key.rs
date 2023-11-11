//! Extended public keys
//!
use crate::{
    types::*, ChildNumber, DerivationPath, Error, ExtendedKey, ExtendedKeyAttrs, ExtendedPrivateKey, KeyFingerprint, Prefix,
    PrivateKey, PublicKey, PublicKeyBytes, Result, KEY_SIZE,
};
use core::str::FromStr;
use hmac::Mac;

/// Extended public secp256k1 ECDSA verification key.
//#[cfg(feature = "secp256k1")]
//#[cfg_attr(docsrs, doc(cfg(feature = "secp256k1")))]
//pub type XPub = ExtendedPublicKey<k256::ecdsa::VerifyingKey>;

/// Extended public keys derived using BIP32.
///
/// Generic around a [`PublicKey`] type. When the `secp256k1` feature of this
/// crate is enabled, the [`XPub`] type provides a convenient alias for
/// extended ECDSA/secp256k1 public keys.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ExtendedPublicKey<K: PublicKey> {
    /// Derived public key
    pub public_key: K,

    /// Extended key attributes.
    pub attrs: ExtendedKeyAttrs,
}

#[allow(dead_code)]
impl<K> ExtendedPublicKey<K>
where
    K: PublicKey,
{
    /// Obtain the non-extended public key value `K`.
    pub fn public_key(&self) -> &K {
        &self.public_key
    }

    /// Get attributes for this key such as depth, parent fingerprint,
    /// child number, and chain code.
    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        &self.attrs
    }

    /// Compute a 4-byte key fingerprint for this extended public key.
    pub fn fingerprint(&self) -> KeyFingerprint {
        self.public_key().fingerprint()
    }

    /// Derive a child key for a particular [`ChildNumber`].
    pub fn derive_child(&self, child_number: ChildNumber) -> Result<Self> {
        if child_number.is_hardened() {
            // Cannot derive child public keys for hardened `ChildNumber`s
            return Err(Error::ChildNumber);
        }

        let depth = self.attrs.depth.checked_add(1).ok_or(Error::Depth)?;

        let mut hmac = HmacSha512::new_from_slice(&self.attrs.chain_code).map_err(Error::Hmac)?;

        hmac.update(&self.public_key.to_bytes());
        hmac.update(&child_number.to_bytes());

        let result = hmac.finalize().into_bytes();
        let (child_key, chain_code) = result.split_at(KEY_SIZE);
        let public_key = self.public_key.derive_child(child_key.try_into()?)?;

        let attrs = ExtendedKeyAttrs {
            parent_fingerprint: self.public_key.fingerprint(),
            child_number,
            chain_code: chain_code.try_into()?,
            depth,
        };

        Ok(ExtendedPublicKey { public_key, attrs })
    }

    pub fn derive_path(self, path: DerivationPath) -> Result<Self> {
        path.iter().try_fold(self, |key, child_num| key.derive_child(child_num))
    }

    /// Serialize the raw public key as a byte array (e.g. SEC1-encoded).
    pub fn to_bytes(&self) -> PublicKeyBytes {
        self.public_key.to_bytes()
    }

    /// Serialize this key as an [`ExtendedKey`].
    pub fn to_extended_key(&self, prefix: Prefix) -> ExtendedKey {
        ExtendedKey { prefix, attrs: self.attrs.clone(), key_bytes: self.to_bytes() }
    }

    pub fn to_string(&self, prefix: Option<Prefix>) -> String {
        let prefix = prefix.unwrap_or(Prefix::XPUB);
        self.to_extended_key(prefix).to_string()
    }

    pub fn from_public_key(public_key: K, attrs: &ExtendedKeyAttrs) -> Self {
        ExtendedPublicKey { public_key, attrs: attrs.clone() }
    }
}

impl<K> From<&ExtendedPrivateKey<K>> for ExtendedPublicKey<K::PublicKey>
where
    K: PrivateKey,
{
    fn from(xprv: &ExtendedPrivateKey<K>) -> ExtendedPublicKey<K::PublicKey> {
        ExtendedPublicKey { public_key: xprv.private_key().public_key(), attrs: xprv.attrs().clone() }
    }
}

impl<K> FromStr for ExtendedPublicKey<K>
where
    K: PublicKey,
{
    type Err = Error;

    fn from_str(xpub: &str) -> Result<Self> {
        ExtendedKey::from_str(xpub)?.try_into()
    }
}

impl<K> TryFrom<ExtendedKey> for ExtendedPublicKey<K>
where
    K: PublicKey,
{
    type Error = Error;

    fn try_from(extended_key: ExtendedKey) -> Result<ExtendedPublicKey<K>> {
        if extended_key.prefix.is_public() {
            Ok(ExtendedPublicKey { public_key: PublicKey::from_bytes(extended_key.key_bytes)?, attrs: extended_key.attrs.clone() })
        } else {
            Err(Error::Crypto(secp256k1::Error::InvalidPublicKey))
        }
    }
}
