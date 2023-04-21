use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
//use aes::cipher::typenum::Zero;
use argon2::Argon2;
use base64::{engine::general_purpose, Engine as _};
use cfg_if::cfg_if;
use chacha20poly1305::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    Key, XChaCha20Poly1305,
};
use faster_hex::{hex_decode, hex_string};
// use kaspa_bip32::ExtendedKey;
use serde::{de::DeserializeOwned, Serializer};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use workflow_core::runtime;
use zeroize::Zeroize;

const DEFAULT_PATH: &str = "~/.kaspa/wallet.kaspa";

pub use kaspa_wallet_core::account::AccountKind;

pub struct Decrypted<T>(pub(crate) T)
where
    T: Zeroize;
impl<T> Drop for Decrypted<T>
where
    T: Zeroize,
{
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<T> AsRef<T> for Decrypted<T>
where
    T: Zeroize,
{
    fn as_ref(&self) -> &T {
        &self.0
    }
}

pub struct Encrypted {
    payload: Vec<u8>,
}

impl Encrypted {
    pub fn new(payload: Vec<u8>) -> Self {
        Encrypted { payload }
    }

    pub fn decrypt<T>(&self, secret: Secret) -> Result<Decrypted<T>>
    where
        T: Zeroize + DeserializeOwned,
    {
        let t: T = serde_json::from_slice(decrypt(&self.payload, secret)?.as_ref())?;
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
pub struct PrvKeyDataId(pub(crate) [u8; 8]);

impl PrvKeyDataId {
    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }
}

impl Serialize for PrvKeyDataId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0))
    }
}

impl<'de> Deserialize<'de> for PrvKeyDataId {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PubKeyDataId(pub(crate) [u8; 8]);

impl PubKeyDataId {
    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }
}

impl Serialize for PubKeyDataId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0))
    }
}

impl<'de> Deserialize<'de> for PubKeyDataId {
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyData {
    pub id: PrvKeyDataId,
    pub encrypted_mnemonics: Vec<Vec<u8>>,
}

impl Drop for PrvKeyData {
    fn drop(&mut self) {
        self.encrypted_mnemonics.zeroize();
    }
}

impl PrvKeyData {
    pub fn new(encrypted_mnemonics: Vec<Vec<u8>>) -> Self {
        //TODO sort mnemonics
        let first_address_payload = "TODO: create address from first mnemonic".as_bytes();
        let id = PrvKeyDataId::new_from_slice(&first_address_payload[0..8]);
        Self { id, encrypted_mnemonics }
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
    pub pub_key_data: PubKeyData,
    pub prv_key_data_id: Option<PrvKeyDataId>,
}

impl Account {
    pub fn new(
        name: String,
        title: String,
        account_kind: AccountKind,
        pub_key_data: PubKeyData,
        prv_key_data_id: Option<PrvKeyDataId>,
    ) -> Self {
        //TODO drive id from pubkey
        let id = serde_json::to_value(pub_key_data.id).unwrap().to_string();
        Self { id, name, title, account_kind, pub_key_data, prv_key_data_id }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAccount {
    pub id: String,
    pub name: String,
    pub title: String,
    pub account_kind: AccountKind,
    pub pub_key_data: PubKeyData,
}

impl OpenAccount {
    pub fn new(name: String, title: String, account_kind: AccountKind, pub_key_data: PubKeyData) -> Self {
        //TODO drive id from pubkey
        let id = serde_json::to_value(pub_key_data.id).unwrap().to_string();
        Self { id, pub_key_data, account_kind, name, title }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub keydata: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub payload: Payload,
    pub accounts: Vec<Account>,
    //pub pub_key_data: Vec<PubKeyData>,
    //pub accounts: Vec<OpenAccount>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
struct WalletData {
    pub payload: Vec<u8>,
    pub accounts: Vec<Account>,
    //pub pub_key_data: Vec<PubKeyData>,
    //pub accounts: Vec<OpenAccount>,
}

impl WalletData {
    fn new(secret: Secret, wallet: Wallet) -> Result<Self> {
        let json = serde_json::to_value(wallet.payload).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
        let payload = encrypt(json.as_bytes(), secret)?;
        Ok(Self {
            payload,
            //pub_key_data: wallet.pub_key_data,
            accounts: wallet.accounts,
        })
    }

    fn create_wallet(&self, secret: Option<Secret>) -> Result<Wallet> {
        let mut wallet = Wallet {
            payload: Payload::default(),
            //pub_key_data: self.pub_key_data.clone(),
            accounts: self.accounts.clone(),
        };

        if let Some(secret) = secret {
            let data = decrypt(&self.payload, secret)?;
            let payload: Payload = serde_json::from_slice(data.as_ref())?;
            wallet.payload = payload;
        }

        Ok(wallet)
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

    pub async fn try_load(&self, secret: Option<Secret>) -> Result<Wallet> {
        if self.exists().await? {
            let buffer = self.read().await.map_err(|err| format!("unable to read wallet file: {err}"))?;
            let wallet_data: WalletData = serde_json::from_slice(buffer.as_ref())?;

            // let data = decrypt(&wallet_data.payload, secret)?;
            // let wallet: Wallet = serde_json::from_slice(data.as_ref())?;
            Ok(wallet_data.create_wallet(secret)?)
        } else {
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store(&self, secret: Secret, wallet: Wallet) -> Result<()> {
        // let json = serde_json::to_value(wallet.payload).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
        // let data = encrypt(json.as_bytes(), secret)?;
        let data =
            serde_json::to_value(WalletData::new(secret, wallet)?).map_err(|err| format!("unable to serialize wallet data: {err}"))?;
        self.write(data.to_string().as_bytes()).await.map_err(|err| format!("unable to read wallet file: {err}"))?;

        Ok(())
    }

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
    //     let wallet = self.try_load(secret).await?;
    //     Ok(wallet.accounts)
    // }

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

#[wasm_bindgen(js_name = "sha256")]
pub fn js_sha256_hash(data: JsValue) -> Result<String> {
    let data = data.try_as_vec_u8()?;
    let hash = sha256_hash(&data)?;
    Ok(hash.as_ref().to_hex())
}

#[wasm_bindgen(js_name = "argon2sha256iv")]
pub fn js_argon2_sha256iv_phash(data: JsValue, byte_length: usize) -> Result<String> {
    let data = data.try_as_vec_u8()?;
    let hash = argon2_sha256iv_hash(&data, byte_length)?;
    Ok(hash.as_ref().to_hex())
}

pub fn sha256_hash(data: &[u8]) -> Result<Secret> {
    let mut sha256 = Sha256::new();
    sha256.update(data);
    Ok(Secret::new(sha256.finalize().to_vec()))
}

pub fn argon2_sha256iv_hash(data: &[u8], byte_length: usize) -> Result<Secret> {
    let salt = sha256_hash(data)?;
    let mut key = vec![0u8; byte_length];
    Argon2::default().hash_password_into(data, salt.as_ref(), &mut key)?;
    Ok(key.into())
}

#[wasm_bindgen(js_name = "encrypt")]
pub fn js_encrypt(text: String, password: String) -> Result<String> {
    let secret = sha256_hash(password.as_bytes())?;
    let encrypted = encrypt(text.as_bytes(), secret)?;
    Ok(general_purpose::STANDARD.encode(encrypted))
}

pub fn encrypt(data: &[u8], secret: Secret) -> Result<Vec<u8>> {
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

#[wasm_bindgen(js_name = "decrypt")]
pub fn js_decrypt(text: String, password: String) -> Result<String> {
    let secret = sha256_hash(password.as_bytes())?;
    let encrypted = decrypt(text.as_bytes(), secret)?;
    let decoded = general_purpose::STANDARD.decode(encrypted)?;
    Ok(String::from_utf8(decoded)?)
}

pub fn decrypt(data: &[u8], secret: Secret) -> Result<Secret> {
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
    fn test_wallet_store_argon2() {
        println!("testing argon2 hash");
        let password = b"user_password";
        let hash = argon2_sha256iv_hash(password, 32).unwrap();
        let hash_hex = hash.as_ref().to_hex();
        // println!("argon2hash: {:?}", hash_hex);
        assert_eq!(hash_hex, "a79b661f0defd1960a4770889e19da0ce2fde1e98ca040f84ab9b2519ca46234");
    }

    #[test]
    fn test_wallet_store_encrypt_decrypt() -> Result<()> {
        println!("testing encrypt/decrypt");

        let password = b"password";
        let original = b"hello world".to_vec();
        // println!("original: {}", original.to_hex());
        let encrypted = encrypt(&original, password.as_ref().into()).unwrap();
        // println!("encrypted: {}", encrypted.to_hex());
        let decrypted = decrypt(&encrypted, password.as_ref().into()).unwrap();
        // println!("decrypted: {}", decrypted.to_hex());
        assert_eq!(decrypted.as_ref(), original);

        Ok(())
    }

    #[tokio::test]
    async fn test_wallet_store_wallet_store_load() -> Result<()> {
        // This test creates a fake instance of keydata, stored account
        // instance and a wallet instance that owns them.  It then tests
        // loading of account references and a wallet instance and confirms
        // that the serialized data is as expected.

        let store = Store::new(Some("test-wallet-store"))?;

        let mut w1 = Wallet::default();

        // let private_key = PrivateKey::from_base58(
        //     "xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi",
        // )?;

        let global_password = "ABC-L4LXw2F7HEK3wJU-Rk4stbPy6c";
        let password = "test-123-# L4LXw2F7HEK3wJU Rk4stbPy6c";
        let mnemonic = "caution guide valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light nut invite fiction visa hamster coyote".to_string();

        let encrypted_mnemonic = encrypt(&mnemonic.as_bytes(), password.as_bytes().into()).unwrap();

        let prv_key_data = PrvKeyData::new(vec![encrypted_mnemonic.clone()]);
        let pub_key_data = PubKeyData::new(vec!["xyz".to_string()]);
        println!("keydata id: {:?}", prv_key_data.id);
        //assert_eq!(prv_key_data.id.0, [79, 36, 5, 159, 220, 113, 179, 22]);

        let account = Account::new(
            "Wallet-A".to_string(),
            "Wallet A".to_string(),
            AccountKind::Bip32,
            pub_key_data.clone(),
            Some(prv_key_data.id),
        );
        w1.payload.keydata.push(prv_key_data);
        w1.payload.accounts.push(account);

        // let account = OpenAccount::new("Open-Account".to_string(), "Open Account".to_string(), AccountKind::Bip32, pub_key_data);
        // w1.accounts.push(account);

        let account = Account::new("Open-Account".to_string(), "Open Account".to_string(), AccountKind::Bip32, pub_key_data, None);
        w1.accounts.push(account);
        // println!("w1: {:?}", w1);

        // store the wallet
        let w1s = serde_json::to_string(&w1).unwrap();
        // println!("w1s: {}", w1s);
        store.try_store(global_password.as_bytes().into(), w1).await?;

        let open_wallet = store.try_load(None).await?;

        println!("\n=========\nopen_wallet.accounts: {:?}\n\n", open_wallet.accounts);

        // load a new instance of the wallet from the store
        let w2 = store.try_load(Some(global_password.as_bytes().into())).await?;
        // purge the store
        store.purge()?;

        let w2s = serde_json::to_string(&w2).unwrap();
        assert_eq!(w1s, w2s);

        let keydata = w2.payload.keydata.get(0).unwrap();
        let first_encrypted_mnemonic = &keydata.encrypted_mnemonics[0];

        println!("first_encrypted_mnemonic: {}", first_encrypted_mnemonic.to_hex());
        assert_eq!(&encrypted_mnemonic, first_encrypted_mnemonic);

        let mn = decrypt(first_encrypted_mnemonic.as_ref(), password.as_bytes().into())?;
        //println!("mn: {:?}", mn.as_ref());
        let mnemonic2 = String::from_utf8(mn.as_ref().into()).unwrap();
        println!("mnemonic: {}", mnemonic2);
        assert_eq!(mnemonic, mnemonic2);
        Ok(())
    }
}
