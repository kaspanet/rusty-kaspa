use crate::{imports::*, encryption::sha256_hash};
use crate::result::Result;
use crate::secret::Secret;
use base64::{engine::general_purpose, Engine as _};
use cfg_if::cfg_if;
use faster_hex::{hex_decode, hex_string};
use serde::Serializer;
use std::path::PathBuf;
use workflow_core::runtime;
use zeroize::Zeroize;

use crate::encryption::{Decrypted, Encryptable, Encrypted};

const DEFAULT_PATH: &str = "~/.kaspa/wallet.kaspa";

pub use kaspa_wallet_core::account::AccountKind;

#[derive(Clone)]
struct PrivateKey(pub(crate) kaspa_bip32::ExtendedKey);

impl PrivateKey {
    #[allow(dead_code)] //TODO : dead_code
    pub fn from_base58(base58: &str) -> Result<Self> {
        let xprv = base58.parse::<kaspa_bip32::ExtendedKey>()?;
        Ok(PrivateKey(xprv))
    }
}

impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
        let xprv = s.parse::<kaspa_bip32::ExtendedKey>().map_err(serde::de::Error::custom)?;
        Ok(PrivateKey(xprv))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyDataId(pub(crate) [u8; 8]);

impl KeyDataId {
    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }
}

impl Serialize for KeyDataId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0))
    }
}

impl<'de> Deserialize<'de> for KeyDataId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <std::string::String as Deserialize>::deserialize(deserializer)?;
        let mut data = vec![0u8; s.len() / 2];
        hex_decode(s.as_bytes(), &mut data).map_err(serde::de::Error::custom)?;
        Ok(Self::new_from_slice(&data))
    }
}

impl Zeroize for KeyDataId {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

type PrvKeyDataId = KeyDataId;
type PubKeyDataId = KeyDataId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDataPayload {
    pub mnemonic: String,
}

impl KeyDataPayload {
    pub fn new(mnemonic: String) -> Self {
        Self { mnemonic }
    }

    pub fn id(&self)->PrvKeyDataId{
        PrvKeyDataId::new_from_slice(&sha256_hash(self.mnemonic.as_bytes()).unwrap().as_ref()[0..8])
    }
}

impl Zeroize for KeyDataPayload {
    fn zeroize(&mut self) {
        self.mnemonic.zeroize();
        // self.mnemonics.zeroize();
        // TODO
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyData {
    pub id: PrvKeyDataId,
    pub payload: Encryptable<KeyDataPayload>,
}

impl Zeroize for PrvKeyData {
    fn zeroize(&mut self) {
        self.id.zeroize();
        // self.mnemonics.zeroize();
        // TODO
    }
}

impl Drop for PrvKeyData {
    fn drop(&mut self) {
        // TODO

        // self.encrypted_mnemonics.zeroize();
    }
}

impl PrvKeyData {
    pub fn new(id: PrvKeyDataId, payload: Encryptable<KeyDataPayload>) -> Self {
        Self { id, payload }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubKeyData {
    pub id: PubKeyDataId,
    pub keys: Vec<String>,
}

impl Drop for PubKeyData {
    fn drop(&mut self) {
        self.keys.zeroize();
    }
}

impl PubKeyData {
    pub fn new(keys: Vec<String>) -> Self {
        //TODO sort keys
        let first_address_payload = "TODO: create address from first mnemonic".as_bytes();
        let id = PubKeyDataId::new_from_slice(&first_address_payload[0..8]);
        Self { id, keys }
    }
}

// AccountReference contains all account data except keydata,
// referring to the Keydata by `id`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub title: String,
    pub account_kind: AccountKind,
    pub is_visible: bool,
    pub pub_key_data: PubKeyData,
    pub prv_key_data_id: Option<PrvKeyDataId>,
}

impl Account {
    pub fn new(
        name: String,
        title: String,
        account_kind: AccountKind,
        is_visible: bool,
        pub_key_data: PubKeyData,
        prv_key_data_id: Option<PrvKeyDataId>,
    ) -> Self {
        //TODO drive id from pubkey
        let id = serde_json::to_value(pub_key_data.id).unwrap().to_string();
        Self { id, name, title, account_kind, pub_key_data, prv_key_data_id, is_visible }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub title: String,
    pub account_kind: AccountKind,
    pub pub_key_data: PubKeyData,
}

impl From<Account> for Metadata {
    fn from(account: Account) -> Self {
        Self {
            id: account.id,
            name: account.name,
            title: account.title,
            account_kind: account.account_kind,
            pub_key_data: account.pub_key_data,
        }
    }
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct OpenAccount {
//     pub id: String,
//     pub name: String,
//     pub title: String,
//     pub account_kind: AccountKind,
//     pub pub_key_data: PubKeyData,
// }

// impl OpenAccount {
//     pub fn new(name: String, title: String, account_kind: AccountKind, pub_key_data: PubKeyData) -> Self {
//         //TODO drive id from pubkey
//         let id = serde_json::to_value(pub_key_data.id).unwrap().to_string();
//         Self { id, pub_key_data, account_kind, name, title }
//     }
// }

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub keydata: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
}
impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.keydata.iter_mut().for_each(Zeroize::zeroize);
        // TODO
        // self.keydata.zeroize();
        // self.accounts.zeroize();
    }
}

// #[derive(Default, Clone, Serialize, Deserialize)]
#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub payload: Encrypted, //Payload,
    pub metadata: Vec<Metadata>,
    //pub pub_key_data: Vec<PubKeyData>,
    //pub accounts: Vec<OpenAccount>,
}
// impl DefaultIsZeroes for Payload {

// }
impl Wallet {
    pub fn try_new(secret: Secret, payload: Payload) -> Result<Self> {
        //TODO drive id from pubkey
        // let id = serde_json::to_value(payload.id).unwrap().to_string();
        // Self { payload, accounts: vec![] }

        //
        let metadata = payload.accounts.iter().filter(|account| account.is_visible).map(|account| account.clone().into()).collect();
        let payload = Decrypted::new(payload).encrypt(secret)?;
        Ok(Self { payload, metadata })
    }

    pub fn payload(&self, secret: Secret) -> Result<Decrypted<Payload>> {
        self.payload.decrypt::<Payload>(secret)
    }
}
// pub struct

// #[derive(Default, Clone, Serialize, Deserialize)]
// struct WalletData {
//     pub payload: Vec<u8>,
//     pub accounts: Vec<Account>,
//     //pub pub_key_data: Vec<PubKeyData>,
//     //pub accounts: Vec<OpenAccount>,
// }

// impl WalletData {
//     fn new(secret: Secret, wallet: Wallet) -> Result<Self> {
//         let json = serde_json::to_value(wallet.payload).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
//         let payload = encrypt(json.as_bytes(), secret)?;
//         Ok(Self {
//             payload,
//             //pub_key_data: wallet.pub_key_data,
//             accounts: wallet.accounts,
//         })
//     }

//     fn create_wallet(&self, secret: Option<Secret>) -> Result<Wallet> {
//         let mut wallet = Wallet {
//             payload: Payload::default(),
//             //pub_key_data: self.pub_key_data.clone(),
//             accounts: self.accounts.clone(),
//         };

//         if let Some(secret) = secret {
//             let data = decrypt(&self.payload, secret)?;
//             let payload: Payload = serde_json::from_slice(data.as_ref())?;
//             wallet.payload = payload;
//         }

//         Ok(wallet)
//     }
// }

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
            let filename = resolve_path(filename.unwrap_or(DEFAULT_PATH));
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

    // pub async fn try_load(&self, secret: Option<Secret>) -> Result<Wallet> {
    pub async fn try_load(&self) -> Result<Wallet> {
        if self.exists().await? {
            let buffer = self.read().await.map_err(|err| format!("unable to read wallet file: {err}"))?;
            // let wallet_data: WalletData = serde_json::from_slice(buffer.as_ref())?;
            // let wallet_data: Wallet = serde_json::from_slice(buffer.as_ref())?;
            let wallet: Wallet = serde_json::from_slice(buffer.as_ref())?;
            Ok(wallet)

            // let data = decrypt(&wallet_data.payload, secret)?;
            // let wallet: Wallet = serde_json::from_slice(data.as_ref())?;
            // Ok(wallet_data.create_wallet(secret)?)
        } else {
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store(&self, secret: Secret, payload: Payload) -> Result<()> {
        // let json = serde_json::to_value(wallet.payload).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
        // let data = encrypt(json.as_bytes(), secret)?;

        let wallet = Wallet::try_new(secret, payload)?;

        let data = serde_json::to_value(wallet).map_err(|err| format!("unable to serialize wallet data: {err}"))?;
        self.write(data.to_string().as_bytes()).await.map_err(|err| format!("unable to read wallet file: {err}"))?;

        Ok(())
    }

    // pub async fn try_store(&self, secret: Secret, wallet: Wallet) -> Result<()> {
    //     // let json = serde_json::to_value(wallet.payload).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
    //     // let data = encrypt(json.as_bytes(), secret)?;
    //     let data =
    //         serde_json::to_value(Wallet::new(secret, wallet)?).map_err(|err| format!("unable to serialize wallet data: {err}"))?;
    //     self.write(data.to_string().as_bytes()).await.map_err(|err| format!("unable to read wallet file: {err}"))?;

    //     Ok(())
    // }

    // /// Obtain [`PrvKeyData`] by [`PrvKeyDataId`]
    // pub async fn try_get_prv_key_data(&self, secret: Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
    //     let wallet = self.try_load(secret).await?;
    //     let idx = wallet.payload.prv_key_data.iter().position(|keydata| &keydata.id == prv_key_data_id);
    //     let keydata = idx.map(|idx| wallet.payload.prv_key_data.get(idx).unwrap().clone());
    //     Ok(keydata)
    // }

    // /// Obtain [`PubKeyData`] by [`PubKeyDataId`]
    // pub async fn try_get_pub_key_data(&self, secret: Secret, pub_key_data_id: &PubKeyDataId) -> Result<Option<PubKeyData>> {
    //     let wallet = self.try_load(secret).await?;
    //     let idx = wallet.open_pub_key_data.iter().position(|keydata| &keydata.id == pub_key_data_id);
    //     let keydata = idx.map(|idx| wallet.open_pub_key_data.get(idx).unwrap().clone());
    //     Ok(keydata)
    // }

    // /// Obtain an array of accounts
    // pub async fn get_accounts(&self, secret: Secret) -> Result<Vec<Account>> {
    //     let wallet = self.try_load().await?;
    //     Ok(wallet.accounts)
    // }

    pub async fn wallet(&self) -> Result<Wallet> {
        let wallet = self.try_load().await?;
        Ok(wallet)
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

            #[allow(dead_code)]
            pub fn purge(&self) -> Result<()> {
                std::fs::remove_file(self.filename())?;
                Ok(())
            }
        }
    }
}

pub fn resolve_path(path: &str) -> PathBuf {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_wallet_store_wallet_store_load() -> Result<()> {
        // This test creates a fake instance of keydata, stored account
        // instance and a wallet instance that owns them.  It then tests
        // loading of account references and a wallet instance and confirms
        // that the serialized data is as expected.

        let store = Store::new(Some("test-wallet-store"))?;

        let mut payload = Payload::default();

        // let mut w1 = Wallet::default();

        // let private_key = PrivateKey::from_base58(
        //     "xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi",
        // )?;

        let global_password = Secret::from("ABC-L4LXw2F7HEK3wJU-Rk4stbPy6c");
        let password = Secret::from("test-123-# L4LXw2F7HEK3wJU Rk4stbPy6c");
        let mnemonic1 = "caution guide valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light nut invite fiction visa hamster coyote".to_string();
        let mnemonic2 = "nut invite fiction visa hamster coyote guide caution valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light".to_string();

        // let encrypted_mnemonic = encrypt(&mnemonic.as_bytes(), password.as_bytes().into()).unwrap();
        // let mmm =

        // let prv_key_data = PrvKeyData::new(vec![mnemonic.as_bytes()]);
        // let prv_key_data = PrvKeyData::new(vec![encrypted_mnemonic.clone()]);
        // let prv_key_data = PrvKeyData::new(vec![Encryptable::Plain(mnemonic.as_bytes().to_vec()).encrypt(password.into())?]);
        // let prv_key_data = PrvKeyData::new(vec![Encryptable::Plain(mnemonic.as_bytes().to_vec())]);
        let key_data_payload1 = KeyDataPayload::new(mnemonic1.clone());
        let prv_key_data1 = PrvKeyData::new(key_data_payload1.id(), Encryptable::Plain(key_data_payload1));

        let key_data_payload2 = KeyDataPayload::new(mnemonic2.clone());
        let prv_key_data2 = PrvKeyData::new(key_data_payload2.id(), Encryptable::Plain(key_data_payload2).into_encrypted(password.clone())?);

        let pub_key_data1 = PubKeyData::new(vec!["abc".to_string()]);
        let pub_key_data2 = PubKeyData::new(vec!["xyz".to_string()]);
        println!("keydata1 id: {:?}", prv_key_data1.id);
        //assert_eq!(prv_key_data.id.0, [79, 36, 5, 159, 220, 113, 179, 22]);
        payload.keydata.push(prv_key_data1.clone());
        payload.keydata.push(prv_key_data2.clone());

        let account1 = Account::new(
            "Wallet-A".to_string(),
            "Wallet A".to_string(),
            AccountKind::Bip32,
            true,
            pub_key_data1.clone(),
            Some(prv_key_data1.id),
        );
        payload.accounts.push(account1);

        let account2 = Account::new(
            "Wallet-A".to_string(),
            "Wallet A".to_string(),
            AccountKind::Bip32,
            true,
            pub_key_data2.clone(),
            Some(prv_key_data2.id),
        );
        payload.accounts.push(account2);

        // let account = OpenAccount::new("Open-Account".to_string(), "Open Account".to_string(), AccountKind::Bip32, pub_key_data);
        // w1.accounts.push(account);

        // let account = Account::new("Open-Account".to_string(), "Open Account".to_string(), AccountKind::Bip32,
        // true,
        // pub_key_data, None);
        // let accounts = vec![account];
        // println!("w1: {:?}", w1);

        let payload_json = serde_json::to_string(&payload).unwrap();
        store.try_store(global_password.clone(), payload).await?;

        // store the wallet
        // let w1s = serde_json::to_string(&w1).unwrap();
        // println!("w1s: {}", w1s);
        // store.try_store(global_password.as_bytes().into(), w1).await?;

        // let open_wallet = store.try_load(None).await?;
        // let wallet = store.try_load().await?;
        // let
        
        // load a new instance of the wallet from the store
        // let w2 = store.try_load(Some(global_password.as_bytes().into())).await?;
        let w2 = store.try_load().await?;
        let w2payload = w2.payload.decrypt::<Payload>(global_password.clone()).unwrap();
        println!("\n---\nwallet.metadata (plain): {:#?}\n\n", w2.metadata);
        // let w2payload_json = serde_json::to_string(w2payload.as_ref()).unwrap();
        println!("\n---nwallet.payload (decrypted): {:#?}\n\n", w2payload.as_ref());
        // purge the store
        store.purge()?;

        // let w2s = serde_json::to_string(&w2).unwrap();
        // assert_eq!(w1s, w2s);
        assert_eq!(payload_json, serde_json::to_string(w2payload.as_ref())?);

        let w2keydata1 = w2payload.as_ref().keydata.get(0).unwrap();
        let w2keydata1_payload = w2keydata1.payload.decrypt(None).unwrap();
        let first_mnemonic = &w2keydata1_payload.as_ref().mnemonic;
        // println!("first mnemonic (plain): {}", hex_string(first_mnemonic.as_ref()));
        println!("first mnemonic (plain): {first_mnemonic}");
        assert_eq!(&mnemonic1, first_mnemonic);

        let w2keydata2 = w2payload.as_ref().keydata.get(1).unwrap();
        let w2keydata2_payload = w2keydata2.payload.decrypt(Some(password.clone())).unwrap();
        let second_mnemonic = &w2keydata2_payload.as_ref().mnemonic;
        println!("second mnemonic (encrypted): {second_mnemonic}");
        assert_eq!(&mnemonic2, second_mnemonic);

        // let mn = decrypt(first_encrypted_mnemonic.as_ref(), password.as_bytes().into())?;
        //println!("mn: {:?}", mn.as_ref());
        // let mnemonic2 = String::from_utf8(mn.as_ref().into()).unwrap();
        // println!("mnemonic: {}", mnemonic2);
        // assert_eq!(mnemonic, mnemonic2);
        Ok(())
    }
}
