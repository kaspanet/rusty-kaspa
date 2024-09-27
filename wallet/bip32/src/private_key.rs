use crate::types::*;
use crate::PublicKey;
use crate::Result;
pub use secp256k1::SecretKey;
use secp256k1::{scalar::Scalar, Secp256k1, SignOnly};

/// Trait for private key types which can be derived using BIP32.
pub trait PrivateKey: Sized {
    /// Public key type which corresponds to this private key.
    type PublicKey: PublicKey;

    /// Initialize this key from bytes.
    fn from_bytes(bytes: &PrivateKeyBytes) -> Result<Self>;

    /// Serialize this key as bytes.
    fn to_bytes(&self) -> PrivateKeyBytes;

    /// Derive a child key from a parent key and the a provided tweak value,
    /// i.e. where `other` is referred to as "I sub L" in BIP32 and sourced
    /// from the left half of the HMAC-SHA-512 output.
    fn derive_child(&self, other: PrivateKeyBytes) -> Result<Self>;

    /// Get the [`Self::PublicKey`] that corresponds to this private key.
    fn public_key(&self) -> Self::PublicKey;
}

impl PrivateKey for SecretKey {
    type PublicKey = secp256k1::PublicKey;

    fn from_bytes(bytes: &PrivateKeyBytes) -> Result<Self> {
        Ok(SecretKey::from_slice(bytes)?)
    }

    fn to_bytes(&self) -> PrivateKeyBytes {
        *self.as_ref()
    }

    fn derive_child(&self, other: PrivateKeyBytes) -> Result<Self> {
        Ok((*self).add_tweak(&Scalar::from_be_bytes(other)?)?)
    }

    fn public_key(&self) -> Self::PublicKey {
        let engine = Secp256k1::<SignOnly>::signing_only();
        secp256k1::PublicKey::from_secret_key(&engine, self)
    }
}
