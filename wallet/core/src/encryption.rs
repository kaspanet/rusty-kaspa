use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use argon2::Argon2;
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    Key, XChaCha20Poly1305,
};
use faster_hex::{hex_decode, hex_string};
use serde::{de::DeserializeOwned, Serializer};
use sha2::{Digest, Sha256};
use std::ops::{Deref, DerefMut};
use zeroize::Zeroize;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "encryptable", content = "payload")]
pub enum Encryptable<T> {
    #[serde(rename = "plain")]
    Plain(T),
    #[serde(rename = "xchacha20poly1305")]
    XChaCha20Poly1305(Encrypted),
}

impl<T> Zeroize for Encryptable<T>
where
    T: Zeroize,
{
    fn zeroize(&mut self) {
        match self {
            Self::Plain(t) => t.zeroize(),
            Self::XChaCha20Poly1305(e) => e.zeroize(),
        }
    }
}

impl<T> Encryptable<T>
where
    T: Clone + Serialize + DeserializeOwned + Zeroize,
{
    pub fn is_encrypted(&self) -> bool {
        !matches!(self, Self::Plain(_))
    }

    pub fn decrypt(&self, secret: Option<&Secret>) -> Result<Decrypted<T>> {
        match self {
            Self::Plain(v) => Ok(Decrypted::new(v.clone())),
            Self::XChaCha20Poly1305(v) => {
                if let Some(secret) = secret {
                    Ok(v.decrypt(secret)?)
                } else {
                    Err("decrypted() secret is 'None' when the data is encryted!".into())
                }
            }
        }
    }

    pub fn encrypt(&self, secret: &Secret) -> Result<Encrypted> {
        match self {
            Self::Plain(v) => Ok(Decrypted::new(v.clone()).encrypt(secret)?),
            Self::XChaCha20Poly1305(v) => Ok(v.clone()),
        }
    }

    pub fn into_encrypted(&self, secret: &Secret) -> Result<Self> {
        match self {
            Self::Plain(v) => Ok(Self::XChaCha20Poly1305(Decrypted::new(v.clone()).encrypt(secret)?)),
            Self::XChaCha20Poly1305(v) => Ok(Self::XChaCha20Poly1305(v.clone())),
        }
    }

    pub fn into_decrypted(self, secret: &Secret) -> Result<Self> {
        match self {
            Self::Plain(v) => Ok(Self::Plain(v)),
            Self::XChaCha20Poly1305(v) => Ok(Self::Plain(v.decrypt::<T>(secret)?.clone())),
        }
    }
}

impl<T> From<T> for Encryptable<T> {
    fn from(value: T) -> Self {
        Encryptable::Plain(value)
    }
}

pub struct Decrypted<T>(pub(crate) T);

impl<T> AsRef<T> for Decrypted<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> Deref for Decrypted<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> DerefMut for Decrypted<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> AsMut<T> for Decrypted<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> Decrypted<T>
where
    T: Serialize,
{
    pub fn new(value: T) -> Self {
        Self(value)
    }

    pub fn encrypt(&self, secret: &Secret) -> Result<Encrypted> {
        let json = serde_json::to_string(&self.0)?;
        let encrypted = encrypt_xchacha20poly1305(json.as_bytes(), secret)?;
        Ok(Encrypted::new(encrypted))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Encrypted {
    payload: Vec<u8>,
}

impl Zeroize for Encrypted {
    fn zeroize(&mut self) {
        self.payload.zeroize();
    }
}

impl Encrypted {
    pub fn new(payload: Vec<u8>) -> Self {
        Encrypted { payload }
    }

    pub fn replace(&mut self, from: Encrypted) {
        self.payload = from.payload;
    }

    pub fn decrypt<T>(&self, secret: &Secret) -> Result<Decrypted<T>>
    where
        T: DeserializeOwned,
    {
        let t: T = serde_json::from_slice(decrypt_xchacha20poly1305(&self.payload, secret)?.as_ref())?;
        Ok(Decrypted(t))
    }
}

impl Serialize for Encrypted {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.payload))
    }
}

impl<'de> Deserialize<'de> for Encrypted {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
        let mut data = vec![0u8; s.len() / 2];
        hex_decode(s.as_bytes(), &mut data).map_err(serde::de::Error::custom)?;
        Ok(Self::new(data))
    }
}

#[wasm_bindgen(js_name = "sha256")]
pub fn js_sha256_hash(data: JsValue) -> Result<String> {
    let data = data.try_as_vec_u8()?;
    let hash = sha256_hash(&data);
    Ok(hash.as_ref().to_hex())
}

#[wasm_bindgen(js_name = "sha256d")]
pub fn js_sha256d_hash(data: JsValue) -> Result<String> {
    let data = data.try_as_vec_u8()?;
    let hash = sha256d_hash(&data);
    Ok(hash.as_ref().to_hex())
}

#[wasm_bindgen(js_name = "argon2sha256iv")]
pub fn js_argon2_sha256iv_phash(data: JsValue, byte_length: usize) -> Result<String> {
    let data = data.try_as_vec_u8()?;
    let hash = argon2_sha256iv_hash(&data, byte_length)?;
    Ok(hash.as_ref().to_hex())
}

pub fn sha256_hash(data: &[u8]) -> Secret {
    let mut sha256 = Sha256::new();
    sha256.update(data);
    Secret::new(sha256.finalize().to_vec())
}

pub fn sha256d_hash(data: &[u8]) -> Secret {
    let mut sha256 = Sha256::new();
    sha256.update(data);
    sha256_hash(sha256.finalize().as_slice())
}

pub fn argon2_sha256iv_hash(data: &[u8], byte_length: usize) -> Result<Secret> {
    let salt = sha256_hash(data);
    let mut key = vec![0u8; byte_length];
    Argon2::default().hash_password_into(data, salt.as_ref(), &mut key)?;
    Ok(key.into())
}

#[wasm_bindgen(js_name = "encryptXChaCha20Poly1305")]
pub fn js_encrypt_xchacha20poly1305(text: String, password: String) -> Result<String> {
    let secret = sha256_hash(password.as_bytes());
    let encrypted = encrypt_xchacha20poly1305(text.as_bytes(), &secret)?;
    Ok(general_purpose::STANDARD.encode(encrypted))
}

pub fn encrypt_xchacha20poly1305(data: &[u8], secret: &Secret) -> Result<Vec<u8>> {
    let private_key_bytes = argon2_sha256iv_hash(secret.as_ref(), 32)?;
    let key = Key::from_slice(private_key_bytes.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng); // 96-bits; unique per message
    let mut buffer = data.to_vec();
    buffer.reserve(16);
    cipher.encrypt_in_place(&nonce, &[], &mut buffer)?;
    buffer.splice(0..0, nonce.iter().cloned());
    Ok(buffer)
}

#[wasm_bindgen(js_name = "decryptXChaCha20Poly1305")]
pub fn js_decrypt_xchacha20poly1305(text: String, password: String) -> Result<String> {
    let secret = sha256_hash(password.as_bytes());
    let encrypted = decrypt_xchacha20poly1305(text.as_bytes(), &secret)?;
    let decoded = general_purpose::STANDARD.decode(encrypted)?;
    Ok(String::from_utf8(decoded)?)
}

pub fn decrypt_xchacha20poly1305(data: &[u8], secret: &Secret) -> Result<Secret> {
    let private_key_bytes = argon2_sha256iv_hash(secret.as_ref(), 32)?;
    let key = Key::from_slice(private_key_bytes.as_ref());
    let cipher = XChaCha20Poly1305::new(key);
    let nonce = &data[0..24];
    let mut buffer = data[24..].to_vec();
    cipher.decrypt_in_place(nonce.into(), &[], &mut buffer)?;
    Ok(Secret::new(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wallet_argon2() {
        println!("testing argon2 hash");
        let password = b"user_password";
        let hash = argon2_sha256iv_hash(password, 32).unwrap();
        let hash_hex = hash.as_ref().to_hex();
        // println!("argon2hash: {:?}", hash_hex);
        assert_eq!(hash_hex, "a79b661f0defd1960a4770889e19da0ce2fde1e98ca040f84ab9b2519ca46234");
    }

    #[test]
    fn test_wallet_encrypt_decrypt() -> Result<()> {
        println!("testing encrypt/decrypt");

        let password = b"password";
        let original = b"hello world".to_vec();
        // println!("original: {}", original.to_hex());
        let password = Secret::new(password.to_vec());
        let encrypted = encrypt_xchacha20poly1305(&original, &password).unwrap();
        // println!("encrypted: {}", encrypted.to_hex());
        let decrypted = decrypt_xchacha20poly1305(&encrypted, &password).unwrap();
        // println!("decrypted: {}", decrypted.to_hex());
        assert_eq!(decrypted.as_ref(), original);

        Ok(())
    }
}
