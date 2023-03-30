use crate::error::Error;
use crate::result::Result;
#[allow(unused_imports)]
use base64::{engine::general_purpose, Engine as _};
use borsh::{BorshDeserialize, BorshSerialize};
use cfg_if::cfg_if;
use chacha20poly1305::Key as ChaChaKey;
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    // aead::{AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305,
    // Nonce,
};
// use heapless::Vec as HeaplessVec;
use kaspa_bip32::SecretKey;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use wasm_bindgen::prelude::*;
#[allow(unused_imports)]
use workflow_core::channel::{Channel, Receiver};
use workflow_core::runtime;

const DEFAULT_PATH: &str = "~/.kaspa/wallet.kaspa";

pub use kaspa_wallet_core::account::AccountKind;

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

    pub async fn try_load(&self) -> Result<Wallet> {
        if self.exists().await? {
            let buffer = self.read().await.map_err(|err| format!("unable to read wallet file: {err}"))?;
            let wallet: Wallet = serde_json::from_slice(&buffer)?;
            Ok(wallet)
        } else {
            //Ok(Wallet::default())
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store(&self) -> Result<()> {
        let wallet = Wallet::default();
        let json = serde_json::to_value(wallet).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
        self.write(json.as_bytes()).await.map_err(|err| format!("unable to read wallet file: {err}"))?;

        Ok(())
    }

    cfg_if! {
        if #[cfg(target_arch = "wasm32")] {

            // WASM32 platforms

            /// test if wallet file or localstorage data exists
            pub async fn exists(&self) -> Result<bool> {
                if runtime::is_node() || runtime::is_nw() {
                    todo!()
                } else {
                    let name = self.filename().to_string_lossy().to_string();
                    Ok(local_storage().get_item(&name)?.is_some())
                }
            }

            /// read wallet file or localstorage data
            pub async fn read(&self) -> Result<Vec<u8>> {
                if runtime::is_node() || runtime::is_nw() {
                    todo!()
                } else {
                    let name = self.filename().to_string_lossy().to_string();
                    let v = local_storage().get_item(&name)?.unwrap();
                    Ok(general_purpose::STANDARD.decode(v)?)
                }
            }

            /// write wallet file or localstorage data
            pub async fn write(&self, data: &[u8]) -> Result<()> {
                if runtime::is_node() || runtime::is_nw() {
                    todo!()
                } else {
                    let v = general_purpose::STANDARD.encode(data);
                    let name = self.filename().to_string_lossy().to_string();
                    local_storage().set_item(&name, &v)?;
                    Ok(())
                }
            }

        } else {

            // native (desktop) platforms

            pub async fn exists(&self) -> Result<bool> {
                Ok(self.filename().exists())
            }

            pub async fn read(&self) -> Result<Vec<u8>> {
                Ok(fs::read(self.filename())?)
            }

            pub async fn write(&self, data: &[u8]) -> Result<()> {
                Ok(fs::write(self.filename(), data)?)
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

pub fn encrypt(data: &mut [u8], _password_hash: &[u8]) -> Result<Vec<u8>> {
    let private_key_bytes = [0u8; 32]; // replace with your actual private key bytes
    let key = ChaChaKey::from_slice(&private_key_bytes);
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng); // 96-bits; unique per message

    let mut buffer = data.to_vec();
    cipher.encrypt_in_place(&nonce, b"", &mut buffer)?;
    // cipher.decrypt_in_place(&nonce, b"", &mut buffer)?;
    Ok(buffer)
}

pub fn decrypt(data: &[u8], _password_hash: &[u8]) -> Result<Vec<u8>> {
    let private_key_bytes = [0u8; 32]; // replace with your actual private key bytes
    let key = ChaChaKey::from_slice(&private_key_bytes);
    let cipher = ChaCha20Poly1305::new(&key);
    let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng); // 96-bits; unique per message

    let mut buffer = data.to_vec();
    // cipher.encrypt_in_place(&nonce, b"", &mut buffer)?;
    cipher.decrypt_in_place(&nonce, b"", &mut buffer)?;
    Ok(buffer)
}
