use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use aes::cipher::typenum::Zero;
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

// pub struct Decrypted<T> {
//     payload : T,
// }

#[derive(Clone)]
pub struct PrivateKey(pub(crate) kaspa_bip32::ExtendedKey);

impl PrivateKey {
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
pub struct KeydataId(pub(crate) [u8; 8]);

impl KeydataId {
    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }
}

impl Serialize for KeydataId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex_string(&self.0))
    }
}

impl<'de> Deserialize<'de> for KeydataId {
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
pub struct Keydata {
    pub id: KeydataId,
    pub private_key: PrivateKey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mnemonic: Option<String>,
    // pub account_kind: AccountKind,
}

impl Drop for Keydata {
    fn drop(&mut self) {
        self.mnemonic.zeroize();
    }
}

impl Keydata {
    pub fn new(
        private_key: PrivateKey,
        mnemonic: Option<String>,
        // account_kind: AccountKind,
    ) -> Self {
        let hash = sha256_hash(&private_key.0.key_bytes).unwrap();
        let id = KeydataId::new_from_slice(&hash.as_ref()[0..8]);

        Self { id, private_key, mnemonic }
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub id: KeydataId,
}

// AccountReference contains all account data except keydata,
// referring to the Keydata by `id`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub keydata_id: KeydataId,
    pub account_kind: AccountKind,
    pub name: String,
    pub title: String,
}

impl Account {
    pub fn new(keydata: &Keydata, account_kind: AccountKind, name: String, title: String) -> Self {
        Self { keydata_id: keydata.id, account_kind, name, title }
    }
}

// pub type WalletAccountList = Arc<Mutex<Vec<Account>>>;

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub keydata: Vec<Keydata>,
    pub accounts: Vec<Account>,
}

impl Payload {
    pub fn new() -> Payload {
        Payload { keydata: vec![], accounts: vec![] }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub payload: Payload,
    pub metadata: Vec<Metadata>,
}

impl Wallet {
    pub fn new() -> Wallet {
        Wallet { payload: Payload::new(), metadata: vec![] }
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

    pub async fn try_load(&self, secret: Secret) -> Result<Wallet> {
        if self.exists().await? {
            let buffer = self.read().await.map_err(|err| format!("unable to read wallet file: {err}"))?;
            let data = decrypt(&buffer, secret)?;
            let wallet: Wallet = serde_json::from_slice(data.as_ref())?;
            Ok(wallet)
        } else {
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store(&self, secret: Secret, wallet: Wallet) -> Result<()> {
        let json = serde_json::to_value(wallet).map_err(|err| format!("unable to serialize wallet data: {err}"))?.to_string();
        let data = encrypt(json.as_bytes(), secret)?;
        self.write(&data).await.map_err(|err| format!("unable to read wallet file: {err}"))?;

        Ok(())
    }

    /// Obtain [`Keydata`] by [`KeydataId`]
    pub async fn try_get_keydata(&self, secret: Secret, keydata_id: &KeydataId) -> Result<Option<Keydata>> {
        let wallet = self.try_load(secret).await?;
        let idx = wallet.payload.keydata.iter().position(|keydata| &keydata.id == keydata_id);
        let keydata = idx.map(|idx| wallet.payload.keydata.get(idx).unwrap().clone());
        Ok(keydata)
    }

    /// Obtain an array of accounts
    pub async fn get_accounts(&self, secret: Secret) -> Result<Vec<Account>> {
        let wallet = self.try_load(secret).await?;
        Ok(wallet.payload.accounts)
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

        let private_key = PrivateKey::from_base58(
            "xprv9s21ZrQH143K3QTDL4LXw2F7HEK3wJUD2nW2nRk4stbPy6cq3jPPqjiChkVvvNKmPGJxWUtg6LnF5kejMRNNU3TGtRBeJgk33yuGBxrMPHi",
        )?;
        let keydata = Keydata::new(private_key, Some("mnemonic".to_string()));
        // println!("keydata id: {:?}", keydata.id);
        assert_eq!(keydata.id.0, [79, 36, 5, 159, 220, 113, 179, 22]);

        let account = Account::new(&keydata, AccountKind::Bip32, "name".to_string(), "title".to_string());
        w1.payload.keydata.push(keydata);
        w1.payload.accounts.push(account);
        // println!("w1: {:?}", w1);

        // store the wallet
        let w1s = serde_json::to_string(&w1).unwrap();
        // println!("w1s: {}", w1s);
        store.try_store(Secret::new(b"password".to_vec()), w1).await?;

        // load a new instance of the wallet from the store
        let w2 = store.try_load(Secret::new(b"password".to_vec())).await?;
        // purge the store
        store.purge()?;

        let w2s = serde_json::to_string(&w2).unwrap();
        assert_eq!(w1s, w2s);

        Ok(())
    }
}
