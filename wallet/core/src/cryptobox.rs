//!
//! Re-export of the `crypto_box` crate that can be used to encrypt and decrypt messages.
//!

use crate::imports::*;
use crypto_box::{
    aead::{Aead, AeadCore, OsRng},
    ChaChaBox,
};
pub use crypto_box::{PublicKey, SecretKey};

///
/// Primitives for encrypting and decrypting messages using the `crypto_box` crate.
/// This exists primarily for the purposes of [WASM bindings](crate::wasm::cryptobox::CryptoBox)
/// to allow access to the `crypto_box` encryption functionality from within web wallets.
///
/// <https://docs.rs/crypto_box/0.9.1/crypto_box/>
///
pub struct CryptoBox {
    public_key: PublicKey,
    codec: ChaChaBox,
}

impl CryptoBox {
    pub fn new(secret_key: &SecretKey, peer_public_key: &PublicKey) -> CryptoBox {
        let cha_cha_box = ChaChaBox::new(peer_public_key, secret_key);
        CryptoBox { public_key: secret_key.public_key(), codec: cha_cha_box }
    }

    pub fn secret_to_public_key(secret_key: &[u8]) -> Vec<u8> {
        let secret_key = SecretKey::from_slice(secret_key).unwrap();
        secret_key.public_key().as_bytes().to_vec()
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let nonce = ChaChaBox::generate_nonce(&mut OsRng);
        Ok(nonce.into_iter().chain(self.codec.encrypt(&nonce, plaintext)?).collect::<Vec<u8>>())
    }

    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 24 + 1 {
            return Err(Error::CipherMessageTooShort);
        }
        Ok(self.codec.decrypt((&ciphertext[0..24]).into(), &ciphertext[24..])?)
    }
}
