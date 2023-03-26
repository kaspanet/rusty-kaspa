use crate::error::Error;
use crate::result::Result;
#[allow(unused_imports)]
use base64::{engine::general_purpose, Engine as _};
use borsh::{BorshDeserialize, BorshSerialize};
use cfg_if::cfg_if;
use kaspa_bip32::SecretKey;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
#[allow(unused_imports)]
use workflow_core::channel::{Channel, Receiver};
use workflow_core::runtime;
use wasm_bindgen::prelude::*;

pub struct PrivateKey(Vec<SecretKey>);

#[derive(Default, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct WalletAccount {
    name: String,
    private_key_index: u32,
}

// pub enum WalletAccountVersion {
//     V1(WalletAccount),
// }

pub type WalletAccountList = Arc<Mutex<Vec<WalletAccount>>>;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub accounts: WalletAccountList,
}

impl Wallet {
    pub fn new() -> Wallet {
        Wallet { accounts: Arc::new(Mutex::new(Vec::new())) }
    }
}

// #[derive()]
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
            let filename = parse(filename.unwrap_or("~/.kaspa/wallet.kaspa"));
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
