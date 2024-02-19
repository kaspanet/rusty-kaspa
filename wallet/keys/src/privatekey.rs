//!
//! Private Key
//!

use crate::imports::*;
use crate::keypair::Keypair;
use js_sys::{Array, Uint8Array};
use workflow_wasm::abi::*;

/// Data structure that envelops a Private Key.
/// @category Wallet SDK
#[derive(Clone, Debug)]
#[wasm_bindgen]
pub struct PrivateKey {
    inner: secp256k1::SecretKey,
}

impl PrivateKey {
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.inner.secret_bytes()
    }
}

impl From<&secp256k1::SecretKey> for PrivateKey {
    fn from(value: &secp256k1::SecretKey) -> Self {
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
    /// Create a new [`PrivateKey`] from a hex-encoded string.
    #[wasm_bindgen(constructor)]
    pub fn try_new(key: &str) -> Result<PrivateKey> {
        Ok(Self { inner: secp256k1::SecretKey::from_str(key)? })
    }
}

impl PrivateKey {
    pub fn try_from_slice(data: &[u8]) -> Result<PrivateKey> {
        Ok(Self { inner: secp256k1::SecretKey::from_slice(data)? })
    }
}

#[wasm_bindgen]
impl PrivateKey {
    /// Returns the [`PrivateKey`] key encoded as a hex string.
    #[wasm_bindgen(js_name = toString)]
    pub fn to_hex(&self) -> String {
        use kaspa_utils::hex::ToHex;
        self.secret_bytes().to_vec().to_hex()
    }

    /// Generate a [`Keypair`] from this [`PrivateKey`].
    #[wasm_bindgen(js_name = toKeypair)]
    pub fn to_keypair(&self) -> Result<Keypair, JsError> {
        Keypair::from_private_key(self)
    }
}

impl TryFrom<JsValue> for PrivateKey {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(hex_str) = js_value.as_string() {
            Self::try_new(hex_str.as_str())
        } else if Array::is_array(&js_value) {
            let array = Uint8Array::new(&js_value);
            Self::try_from_slice(array.to_vec().as_slice())
        } else {
            Ok(ref_from_abi!(PrivateKey, &js_value)?)
        }
    }
}
