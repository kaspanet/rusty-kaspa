use crate::result::Result;
use secp256k1::{PublicKey, Secp256k1, SecretKey, XOnlyPublicKey};
use serde_wasm_bindgen::*;
use std::str::FromStr;
use wasm_bindgen::prelude::*;
use workflow_wasm::abi::TryFromJsValue;

/// Data structure that contains a secret and public keys.
#[derive(Debug, Clone)]
#[wasm_bindgen(inspectable)]
pub struct Keypair {
    #[allow(dead_code)]
    // secret_key should not be exposed
    secret_key: SecretKey,
    public_key: PublicKey,
    xonly_public_key: XOnlyPublicKey,
}

#[wasm_bindgen]
impl Keypair {
    fn new(secret_key: SecretKey, public_key: PublicKey, xonly_public_key: XOnlyPublicKey) -> Self {
        Self { secret_key, public_key, xonly_public_key }
    }

    #[wasm_bindgen(getter = publicKey)]
    pub fn get_public_key(&self) -> JsValue {
        to_value(&self.public_key).unwrap()
    }

    #[wasm_bindgen(getter = privateKey)]
    pub fn get_private_key(&self) -> PrivateKey {
        (&self.secret_key).into()
    }

    #[wasm_bindgen(getter = xOnlyPublicKey)]
    pub fn get_xonly_public_key(&self) -> JsValue {
        to_value(&self.xonly_public_key).unwrap()
    }
}

#[wasm_bindgen]
pub fn generate_random_keypair_not_secure() -> Result<Keypair, JsError> {
    let secp = Secp256k1::new();
    let (secret_key, public_key) = secp.generate_keypair(&mut rand::thread_rng());
    let (xonly_public_key, _) = public_key.x_only_public_key();
    Ok(Keypair::new(secret_key, public_key, xonly_public_key))
}

/// Data structure that envelops a Private Key
#[derive(TryFromJsValue, Clone, Debug)]
#[wasm_bindgen]
pub struct PrivateKey {
    inner: SecretKey,
}

impl PrivateKey {
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.inner.secret_bytes()
    }
}

impl From<&SecretKey> for PrivateKey {
    fn from(value: &SecretKey) -> Self {
        Self { inner: *value }
    }
}

impl From<&PrivateKey> for [u8; 32] {
    fn from(key: &PrivateKey) -> Self {
        key.secret_bytes()
    }
}

#[wasm_bindgen]
impl PrivateKey {
    #[wasm_bindgen(constructor)]
    pub fn new(key: &str) -> Result<PrivateKey> {
        Ok(Self { inner: SecretKey::from_str(key)? })
    }
}
