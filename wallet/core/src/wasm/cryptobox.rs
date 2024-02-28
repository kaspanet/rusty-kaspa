use crate::cryptobox::CryptoBox as NativeCryptoBox;
use crate::imports::*;
use base64::{engine::general_purpose, Engine as _};
use crypto_box::{PublicKey, SecretKey, KEY_SIZE};
use kaspa_wasm_core::types::BinaryT;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "CryptoBoxPrivateKey | HexString | Uint8Array")]
    pub type CryptoBoxPrivateKeyT;

    #[wasm_bindgen(typescript_type = "CryptoBoxPublicKey | HexString | Uint8Array")]
    pub type CryptoBoxPublicKeyT;
}

/// @category Wallet SDK
#[wasm_bindgen]
pub struct CryptoBoxPrivateKey {
    secret_key: SecretKey,
}

#[wasm_bindgen]
impl CryptoBoxPrivateKey {
    #[wasm_bindgen(constructor)]
    #[allow(non_snake_case)]
    pub fn ctor(secretKey: BinaryT) -> Result<CryptoBoxPrivateKey> {
        JsValue::from(secretKey).try_into()
    }

    pub fn to_public_key(&self) -> CryptoBoxPublicKey {
        CryptoBoxPublicKey { public_key: self.secret_key.public_key() }
    }
}

impl TryFrom<JsValue> for CryptoBoxPrivateKey {
    type Error = Error;

    fn try_from(value: JsValue) -> Result<Self> {
        let secret_key = value.try_as_vec_u8()?;
        if secret_key.len() != KEY_SIZE {
            return Err(Error::InvalidPrivateKeyLength);
        }
        Ok(Self { secret_key: SecretKey::from_slice(&secret_key)? })
    }
}

impl std::ops::Deref for CryptoBoxPrivateKey {
    type Target = SecretKey;

    fn deref(&self) -> &Self::Target {
        &self.secret_key
    }
}

/// @category Wallet SDK
#[wasm_bindgen]
pub struct CryptoBoxPublicKey {
    public_key: PublicKey,
}

impl TryFrom<JsValue> for CryptoBoxPublicKey {
    type Error = Error;

    fn try_from(value: JsValue) -> Result<Self> {
        let public_key = value.try_as_vec_u8()?;
        if public_key.len() != KEY_SIZE {
            return Err(Error::InvalidPublicKeyLength);
        }
        Ok(Self { public_key: PublicKey::from_slice(&public_key)? })
    }
}

#[wasm_bindgen]
impl CryptoBoxPublicKey {
    #[wasm_bindgen(constructor)]
    #[allow(non_snake_case)]
    pub fn ctor(publicKey: BinaryT) -> Result<CryptoBoxPublicKey> {
        JsValue::from(publicKey).try_into()
    }

    #[wasm_bindgen(js_name = "toString")]
    pub fn to_string_impl(&self) -> String {
        self.public_key.as_bytes().as_slice().to_hex()
    }

    // #[wasm_bindgen(getter, js_name = "publicKey")]
    // pub fn js_public_key(&self) -> String {
    //     self.public_key.as_bytes().as_slice().to_hex()
    // }
}

impl std::ops::Deref for CryptoBoxPublicKey {
    type Target = PublicKey;

    fn deref(&self) -> &Self::Target {
        &self.public_key
    }
}

///
/// CryptoBox allows for encrypting and decrypting messages using the `crypto_box` crate.
///
/// https://docs.rs/crypto_box/0.9.1/crypto_box/
///
///  @category Wallet SDK
///
#[wasm_bindgen(inspectable)]
pub struct CryptoBox {
    inner: NativeCryptoBox,
}

#[wasm_bindgen]
impl CryptoBox {
    #[wasm_bindgen(constructor)]
    #[allow(non_snake_case)]
    pub fn ctor(secretKey: CryptoBoxPrivateKeyT, peerPublicKey: CryptoBoxPublicKeyT) -> Result<CryptoBox> {
        let secret_key = CryptoBoxPrivateKey::try_from_js_value(secretKey.clone()).or_else(|_| JsValue::from(secretKey).try_into())?;
        let peer_public_key =
            CryptoBoxPublicKey::try_from_js_value(peerPublicKey.clone()).or_else(|_| JsValue::from(peerPublicKey).try_into())?;
        Ok(Self { inner: NativeCryptoBox::new(&secret_key, &peer_public_key) })
    }

    #[wasm_bindgen(getter, js_name = "publicKey")]
    pub fn js_public_key(&self) -> String {
        self.inner.public_key().as_bytes().as_slice().to_hex()
    }

    pub fn encrypt(&self, plaintext: String) -> Result<String> {
        let encrypted = self.inner.encrypt(plaintext.as_bytes())?;
        Ok(general_purpose::STANDARD.encode(encrypted))
    }

    pub fn decrypt(&self, base64string: String) -> Result<String> {
        let bytes = general_purpose::STANDARD.decode(base64string)?;
        let decrypted = self.inner.decrypt(&bytes)?;
        Ok(String::from_utf8(decrypted)?)
    }
}
