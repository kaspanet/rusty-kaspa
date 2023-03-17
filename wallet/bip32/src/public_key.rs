use crate::types::*;
use ripemd::{Digest, Ripemd160};
use secp256k1::{scalar::Scalar, Secp256k1, VerifyOnly};
use sha2::Sha256;

/// Trait for key types which can be derived using BIP32.
pub trait PublicKey: Sized {
    /// Initialize this key from bytes.
    fn from_bytes(bytes: PublicKeyBytes) -> Result<Self>;

    /// Serialize this key as bytes.
    fn to_bytes(&self) -> PublicKeyBytes;

    /// Derive a child key from a parent key and a provided tweak value.
    fn derive_child(&self, other: PrivateKeyBytes) -> Result<Self>;

    /// Compute a 4-byte key fingerprint for this public key.
    ///
    /// Default implementation uses `RIPEMD160(SHA256(public_key))`.
    fn fingerprint(&self) -> KeyFingerprint {
        let digest = Ripemd160::digest(Sha256::digest(self.to_bytes()));
        digest[..4].try_into().expect("digest truncated")
    }
}

impl PublicKey for secp256k1::PublicKey {
    fn from_bytes(bytes: PublicKeyBytes) -> Result<Self> {
        Ok(secp256k1::PublicKey::from_slice(&bytes)?)
    }

    fn to_bytes(&self) -> PublicKeyBytes {
        self.serialize()
    }

    fn derive_child(&self, other: PrivateKeyBytes) -> Result<Self> {
        let engine = Secp256k1::<VerifyOnly>::verification_only();

        let other = Scalar::from_be_bytes(other)?;

        let child_key = *self;
        let child_key = child_key
            .add_exp_tweak(&engine, &other)
            //.add_exp_assign(&engine, &other)
            .map_err(Error::Crypto)?;

        Ok(child_key)
    }
}
