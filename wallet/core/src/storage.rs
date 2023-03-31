use crate::error::Error;
use crate::result::Result;
#[allow(unused_imports)]
use base64::{engine::general_purpose, Engine as _};
use borsh::{BorshDeserialize, BorshSerialize};
use cfg_if::cfg_if;
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    ChaCha20Poly1305, Key,
};
use js_sys::Object;
use kaspa_bip32::SecretKey;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
use workflow_core::runtime;
use zeroize::Zeroize;

const DEFAULT_PATH: &str = "~/.kaspa/wallet.kaspa";

pub use kaspa_wallet_core::account::AccountKind;

pub struct Secret(Vec<u8>);

impl AsRef<[u8]> for Secret {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
impl From<Vec<u8>> for Secret {
    fn from(vec: Vec<u8>) -> Self {
        Secret(vec)
    }
}
impl From<&[u8]> for Secret {
    fn from(slice: &[u8]) -> Self {
        Secret(slice.to_vec())
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize()
    }
}

pub struct PrivateKey(Vec<SecretKey>);

#[derive(Default, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct StoredWalletAccount {
    pub private_key_index: u32,
    pub account_kind: AccountKind,
    pub name: String,
    pub title: String,
}

// pub enum WalletAccountVersion {
//     V1(WalletAccount),
// }

pub type WalletAccountList = Arc<Mutex<Vec<StoredWalletAccount>>>;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub accounts: WalletAccountList,
}

impl Wallet {
    pub fn new() -> Wallet {
        Wallet { accounts: Arc::new(Mutex::new(Vec::new())) }
    }
}

#[wasm_bindgen(module = "fs")]
extern "C" {
    #[wasm_bindgen(js_name = existsSync)]
    pub fn exists_sync(file: &str) -> bool;
    #[wasm_bindgen(js_name = writeFileSync)]
    pub fn write_file_sync(file: &str, data: &str, options: Object);
    #[wasm_bindgen(js_name = readFileSync)]
    pub fn read_file_sync(file: &str, options: Object) -> JsValue;
}

/// Wallet file storage interface
#[wasm_bindgen(inspectable)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Store {
    filename: PathBuf,
}

#[wasm_bindgen]
impl Store {
    #[wasm_bindgen(getter, js_name = filename)]
    pub fn filename_as_string(&self) -> String {
        self.filename.to_str().unwrap().to_string()
    }
}

impl Store {
    pub fn new(filename: Option<&str>) -> Result<Store> {
        let filename = {
            #[allow(clippy::let_and_return)]
            let filename = parse(filename.unwrap_or(DEFAULT_PATH));
            cfg_if! {
                if #[cfg(any(target_os = "linux", target_os = "macos", target_family = "unix", target_os = "windows"))] {
                    filename
                } else if #[cfg(target_arch = "wasm32")] {
                    if runtime::is_node() || runtime::is_nw() {
                        filename
                    } else {
                        PathBuf::from(filename.file_name().ok_or(Error::InvalidFilename(format!("{}",filename.display())))?)
                    }
                }
            }
        };

        Ok(Store { filename })
    }

    pub fn filename(&self) -> &PathBuf {
        &self.filename
    }

    pub async fn try_load(&self, secret: Secret) -> Result<Wallet> {
        if self.exists().await? {
            let buffer = self.read().await.map_err(|err| format!("unable to read wallet file: {err}"))?;
            let data = decrypt(&buffer, secret)?;
            let wallet: Wallet = serde_json::from_slice(&data)?;
            Ok(wallet)
        } else {
            //Ok(Wallet::default())
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store(&self, secret: Secret) -> Result<()> {
        let wallet = Wallet::default();
        let json = serde_json::to_value(wallet).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
        let data = encrypt(json.as_bytes(), secret)?;
        self.write(&data).await.map_err(|err| format!("unable to read wallet file: {err}"))?;

        Ok(())
    }

    cfg_if! {
        if #[cfg(target_arch = "wasm32")] {

            // WASM32 platforms

            /// test if wallet file or localstorage data exists
            pub async fn exists(&self) -> Result<bool> {
                let filename = self.filename().to_string_lossy().to_string();
                if runtime::is_node() || runtime::is_nw() {
                    Ok(exists_sync(&filename))
                } else {
                    Ok(local_storage().get_item(&filename)?.is_some())
                }
            }

            /// read wallet file or localstorage data
            pub async fn read(&self) -> Result<Vec<u8>> {
                let filename = self.filename().to_string_lossy().to_string();
                if runtime::is_node() || runtime::is_nw() {
                    let options = Object::new();
                    // options.set("encoding", "utf-8");
                    let js_value = read_file_sync(&filename, options);
                    let base64enc = js_value.as_string().expect("wallet file data is not a string (empty or encoding error)");
                    Ok(general_purpose::STANDARD.decode(base64enc)?)

                    // let vec : Vec<u8> = js_value.as_str();
                    // Ok(vec)
                } else {
                    let base64enc = local_storage().get_item(&filename)?.unwrap();
                    Ok(general_purpose::STANDARD.decode(base64enc)?)
                }
            }

            /// write wallet file or localstorage data
            pub async fn write(&self, data: &[u8]) -> Result<()> {
                let filename = self.filename().to_string_lossy().to_string();
                let base64enc = general_purpose::STANDARD.encode(data);
                if runtime::is_node() || runtime::is_nw() {
                    // let js_array = Array::new();
                    // js_array.from(data)?;
                    let options = Object::new();
                    write_file_sync(&filename, &base64enc, options);
                } else {
                    local_storage().set_item(&filename, &base64enc)?;
                }
                Ok(())
            }

        } else {

            // native (desktop) platforms

            pub async fn exists(&self) -> Result<bool> {
                Ok(self.filename().exists())
            }

            pub async fn read(&self) -> Result<Vec<u8>> {
                let buffer = std::fs::read(self.filename())?;
                let base64enc = String::from_utf8(buffer)?;
                Ok(general_purpose::STANDARD.decode(base64enc)?)

            }

            pub async fn write(&self, data: &[u8]) -> Result<()> {
                let base64enc = general_purpose::STANDARD.encode(data);

                Ok(std::fs::write(self.filename(), base64enc)?)
            }
        }
    }
}

pub fn parse(path: &str) -> PathBuf {
    if let Some(_stripped) = path.strip_prefix('~') {
        if runtime::is_web() {
            PathBuf::from(path)
        } else if runtime::is_node() || runtime::is_nw() {
            todo!();
        } else {
            cfg_if! {
                if #[cfg(target_arch = "wasm32")] {
                    PathBuf::from(path)
                } else {
                    home::home_dir().unwrap().join(_stripped)
                }
            }
        }
    } else {
        PathBuf::from(path)
    }
}

pub fn local_storage() -> web_sys::Storage {
    web_sys::window().unwrap().local_storage().unwrap().unwrap()
}

#[wasm_bindgen(js_name = "encrypt")]
pub fn js_encrypt(text: String, password: String) -> Result<String> {
    let secret = Secret(hash(&password)?);
    let encrypted = encrypt(text.as_bytes(), secret)?;
    Ok(general_purpose::STANDARD.encode(encrypted))
}

#[wasm_bindgen(js_name = "decrypt")]
pub fn js_decrypt(text: String, password: String) -> Result<String> {
    let secret = Secret(hash(&password)?);
    let encrypted = decrypt(text.as_bytes(), secret)?;
    let decoded = general_purpose::STANDARD.decode(encrypted)?;
    Ok(String::from_utf8(decoded)?)
}

pub fn encrypt(data: &[u8], secret: Secret) -> Result<Vec<u8>> {
    let private_key_bytes: &[u8; 32] = secret.as_ref().try_into()?;
    let key = Key::from_slice(private_key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng); // 96-bits; unique per message
    let mut buffer = data.to_vec();
    cipher.encrypt_in_place(&nonce, b"", &mut buffer)?;
    buffer.splice(0..0, nonce.iter().cloned());
    Ok(buffer)
}

pub fn decrypt(data: &[u8], secret: Secret) -> Result<Vec<u8>> {
    let private_key_bytes: &[u8; 32] = secret.as_ref().try_into()?;
    let key = Key::from_slice(private_key_bytes);
    let cipher = ChaCha20Poly1305::new(key);
    let nonce = &data[0..12];
    let mut buffer = data[12..].to_vec();
    cipher.decrypt_in_place(nonce.into(), b"", &mut buffer)?;
    Ok(buffer)
}

pub fn hash(password: &str) -> Result<Vec<u8>> {
    let mut sha256 = Sha256::new();
    sha256.update(password);
    Ok(sha256.finalize().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        println!("testing encrypt/decrypt");
        let hash = hash("password").unwrap();

        let data = b"hello world".to_vec();
        let orig = data.clone();
        let data = encrypt(&data, hash.as_slice().into()).unwrap();
        let data = decrypt(&data, hash.as_slice().into()).unwrap();
        assert_eq!(data, orig);
    }
}
