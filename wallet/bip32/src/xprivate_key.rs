use hmac::Mac;
use std::fmt::{self, Debug};
use std::str::FromStr;
use subtle::{Choice, ConstantTimeEq};
use zeroize::{Zeroize, Zeroizing};

use crate::{
    result::Result, types::*, ChildNumber, DerivationPath, ExtendedKey, ExtendedKeyAttrs, ExtendedPublicKey, Prefix, PrivateKey,
    PublicKey,
};

/// Derivation domain separator for BIP39 keys.
const BIP39_DOMAIN_SEPARATOR: [u8; 12] = [0x42, 0x69, 0x74, 0x63, 0x6f, 0x69, 0x6e, 0x20, 0x73, 0x65, 0x65, 0x64];

/// Extended private keys derived using BIP32.
///
/// Generic around a [`PrivateKey`] type.
#[derive(Clone)]
pub struct ExtendedPrivateKey<K: PrivateKey> {
    /// Derived private key
    private_key: K,

    /// Extended key attributes.
    attrs: ExtendedKeyAttrs,
}

impl<K> ExtendedPrivateKey<K>
where
    K: PrivateKey,
{
    /// Maximum derivation depth.
    pub const MAX_DEPTH: Depth = u8::MAX;

    /// Create the root extended key for the given seed value.
    pub fn new<S>(seed: S) -> Result<Self>
    where
        S: AsRef<[u8]>,
    {
        if ![16, 32, 64].contains(&seed.as_ref().len()) {
            return Err(Error::SeedLength);
        }

        let mut hmac = HmacSha512::new_from_slice(&BIP39_DOMAIN_SEPARATOR)?;
        hmac.update(seed.as_ref());

        let result = hmac.finalize().into_bytes();
        //println!("seed hash {}", hex::encode(result));
        let (secret_key, chain_code) = result.split_at(KEY_SIZE);
        let private_key = PrivateKey::from_bytes(secret_key.try_into()?)?;
        let attrs = ExtendedKeyAttrs {
            depth: 0,
            parent_fingerprint: KeyFingerprint::default(),
            child_number: ChildNumber::default(),
            chain_code: chain_code.try_into()?,
        };

        Ok(ExtendedPrivateKey { private_key, attrs })
    }

    /// Derive a child key for a particular [`ChildNumber`].
    pub fn derive_child(&self, child_number: ChildNumber) -> Result<Self> {
        let depth = self.attrs.depth.checked_add(1).ok_or(Error::Depth)?;

        let mut hmac = HmacSha512::new_from_slice(&self.attrs.chain_code).map_err(Error::Hmac)?;

        if child_number.is_hardened() {
            hmac.update(&[0]);
            hmac.update(&self.private_key.to_bytes());
        } else {
            hmac.update(&self.private_key.public_key().to_bytes());
        }

        hmac.update(&child_number.to_bytes());

        let result = hmac.finalize().into_bytes();
        let (child_key, chain_code) = result.split_at(KEY_SIZE);

        // We should technically loop here if a `secret_key` is zero or overflows
        // the order of the underlying elliptic curve group, incrementing the
        // index, however per "Child key derivation (CKD) functions":
        // https://github.com/bitcoin/bips/blob/master/bip-0032.mediawiki#child-key-derivation-ckd-functions
        //
        // > "Note: this has probability lower than 1 in 2^127."
        //
        // ...so instead, we simply return an error if this were ever to happen,
        // as the chances of it happening are vanishingly small.
        let private_key = self.private_key.derive_child(child_key.try_into()?)?;

        let attrs = ExtendedKeyAttrs {
            parent_fingerprint: self.private_key.public_key().fingerprint(),
            child_number,
            chain_code: chain_code.try_into()?,
            depth,
        };

        Ok(ExtendedPrivateKey { private_key, attrs })
    }

    pub fn derive_path(self, path: &DerivationPath) -> Result<Self> {
        path.iter().try_fold(self, |key, child_num| key.derive_child(child_num))
    }

    /// Borrow the derived private key value.
    pub fn private_key(&self) -> &K {
        &self.private_key
    }

    /// Serialize the derived public key as bytes.
    pub fn public_key(&self) -> ExtendedPublicKey<K::PublicKey> {
        self.into()
    }

    /// Get attributes for this key such as depth, parent fingerprint,
    /// child number, and chain code.
    pub fn attrs(&self) -> &ExtendedKeyAttrs {
        &self.attrs
    }

    /// Serialize the raw private key as a byte array.
    pub fn to_bytes(&self) -> PrivateKeyBytes {
        self.private_key.to_bytes()
    }

    /// Serialize this key as an [`ExtendedKey`].
    pub fn to_extended_key(&self, prefix: Prefix) -> ExtendedKey {
        // Add leading `0` byte
        let mut key_bytes = [0u8; KEY_SIZE + 1];
        key_bytes[1..].copy_from_slice(&self.to_bytes());

        ExtendedKey { prefix, attrs: self.attrs.clone(), key_bytes }
    }

    pub fn to_string(&self, prefix: Prefix) -> Zeroizing<String> {
        Zeroizing::new(self.to_extended_key(prefix).to_string())
    }
}

impl<K> ConstantTimeEq for ExtendedPrivateKey<K>
where
    K: PrivateKey,
{
    fn ct_eq(&self, other: &Self) -> Choice {
        let mut key_a = self.to_bytes();
        let mut key_b = self.to_bytes();

        let result = key_a.ct_eq(&key_b)
            & self.attrs.depth.ct_eq(&other.attrs.depth)
            & self.attrs.parent_fingerprint.ct_eq(&other.attrs.parent_fingerprint)
            & self.attrs.child_number.0.ct_eq(&other.attrs.child_number.0)
            & self.attrs.chain_code.ct_eq(&other.attrs.chain_code);

        key_a.zeroize();
        key_b.zeroize();

        result
    }
}

impl<K> Debug for ExtendedPrivateKey<K>
where
    K: PrivateKey,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO(tarcieri): use `finish_non_exhaustive` when stable
        f.debug_struct("ExtendedPrivateKey").field("private_key", &"...").field("attrs", &self.attrs).finish()
    }
}

/// NOTE: uses [`ConstantTimeEq`] internally
impl<K> Eq for ExtendedPrivateKey<K> where K: PrivateKey {}

/// NOTE: uses [`ConstantTimeEq`] internally
impl<K> PartialEq for ExtendedPrivateKey<K>
where
    K: PrivateKey,
{
    fn eq(&self, other: &Self) -> bool {
        self.ct_eq(other).into()
    }
}

impl<K> FromStr for ExtendedPrivateKey<K>
where
    K: PrivateKey,
{
    type Err = Error;

    fn from_str(xprv: &str) -> Result<Self> {
        let key = ExtendedKey::from_str(xprv)?;
        key.try_into()
    }
}

impl<K> TryFrom<ExtendedKey> for ExtendedPrivateKey<K>
where
    K: PrivateKey,
{
    type Error = Error;

    fn try_from(extended_key: ExtendedKey) -> Result<ExtendedPrivateKey<K>> {
        if extended_key.prefix.is_private() && extended_key.key_bytes[0] == 0 {
            Ok(ExtendedPrivateKey {
                private_key: PrivateKey::from_bytes(extended_key.key_bytes[1..].try_into()?)?,
                attrs: extended_key.attrs.clone(),
            })
        } else {
            Err(Error::Crypto(secp256k1::Error::InvalidSecretKey))
        }
    }
}
