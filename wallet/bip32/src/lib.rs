use zeroize::Zeroizing;

pub use secp256k1;
pub use secp256k1::SecretKey;

mod private_key;
mod public_key;
mod xkey;
mod xprivate_key;
mod xpublic_key;

mod address_type;
mod attrs;
mod child_number;
mod derivation_path;
mod error;
mod mnemonic;
mod prefix;
mod result;
pub mod types;

pub mod wasm {
    //! WASM bindings for the `bip32` module.
    pub use crate::mnemonic::{Language, Mnemonic, WordCount};
}

pub use address_type::AddressType;
pub use attrs::ExtendedKeyAttrs;
pub use child_number::ChildNumber;
pub use derivation_path::DerivationPath;
pub use mnemonic::{Language, Mnemonic, WordCount};
pub use prefix::Prefix;
pub use private_key::PrivateKey;
pub use public_key::PublicKey;
pub use types::*;
pub use xkey::ExtendedKey;
pub use xprivate_key::ExtendedPrivateKey;
pub use xpublic_key::ExtendedPublicKey;

/// Extension for [`secp256k1::SecretKey`] that provides access
/// to [`secp256k1::PublicKey`] and the public key string representation.
pub trait SecretKeyExt {
    fn get_public_key(&self) -> secp256k1::PublicKey;
    fn as_str(&self, attrs: ExtendedKeyAttrs, prefix: Prefix) -> Zeroizing<String>;
}

impl SecretKeyExt for secp256k1::SecretKey {
    fn get_public_key(&self) -> secp256k1::PublicKey {
        secp256k1::PublicKey::from_secret_key_global(self)
    }
    fn as_str(&self, attrs: ExtendedKeyAttrs, prefix: Prefix) -> Zeroizing<String> {
        // Add leading `0` byte
        let mut key_bytes = [0u8; KEY_SIZE + 1];
        key_bytes[1..].copy_from_slice(&self.to_bytes());

        let key = ExtendedKey { prefix, attrs, key_bytes };

        Zeroizing::new(key.to_string())
    }
}
