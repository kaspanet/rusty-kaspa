/// Size of input key material and derived keys.
pub const KEY_SIZE: usize = 32;

/// Bytes which represent a public key.
///
/// Includes an extra byte for an SEC1 tag.
pub type PublicKeyBytes = [u8; KEY_SIZE + 1];

/// Bytes which represent a private key.
pub type PrivateKeyBytes = [u8; KEY_SIZE];

/// Chain code: extension for both private and public keys which provides an
/// additional 256-bits of entropy.
pub type ChainCode = [u8; KEY_SIZE];

/// Derivation depth.
pub type Depth = u8;

/// BIP32 key fingerprints.
pub type KeyFingerprint = [u8; 4];

/// BIP32 "versions": integer representation of the key prefix.
pub type Version = u32;

pub type HmacSha512 = hmac::Hmac<sha2::Sha512>;

pub use crate::error::*;
