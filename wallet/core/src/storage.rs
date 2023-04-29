use crate::address::create_xpub_from_mnemonic;
use crate::result::Result;
use crate::secret::Secret;
use crate::{encryption::sha256_hash, imports::*};
use faster_hex::{hex_decode, hex_string};
use kaspa_bip32::{ExtendedPublicKey, Mnemonic};
use serde::Serializer;
use std::path::PathBuf;
#[allow(unused_imports)]
use workflow_core::runtime;
use workflow_store::fs;
use zeroize::Zeroize;

pub use crate::encryption::{Decrypted, Encryptable, Encrypted};

pub const DEFAULT_WALLET_FOLDER: &str = "~/.kaspa/";
pub const DEFAULT_WALLET_NAME: &str = "kaspa";
pub const DEFAULT_WALLET_FILE: &str = "~/.kaspa/kaspa.wallet";

pub use kaspa_wallet_core::account::AccountKind;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyDataId(pub(crate) [u8; 8]);

impl KeyDataId {
    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }

    pub fn to_hex(&self) -> String {
        self.0.to_vec().to_hex()
    }
}

impl std::fmt::Debug for KeyDataId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "KeyDataId ( {:?} )", self.0)
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

pub type PrvKeyDataId = KeyDataId;
pub type PubKeyDataId = KeyDataId;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDataPayload {
    pub mnemonic: String,
}

impl KeyDataPayload {
    pub fn new(mnemonic: String) -> Self {
        Self { mnemonic }
    }

    pub fn id(&self) -> PrvKeyDataId {
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

impl PrvKeyData {
    pub async fn create_xpub(
        &self,
        payment_secret: Option<Secret>,
        account_kind: AccountKind,
        account_index: u64,
    ) -> Result<ExtendedPublicKey<secp256k1::PublicKey>> {
        let payload = self.payload.decrypt(payment_secret)?;
        let seed_words = &payload.as_ref().mnemonic;
        create_xpub_from_mnemonic(seed_words, account_kind, account_index).await
    }
}

impl Zeroize for PrvKeyData {
    fn zeroize(&mut self) {
        self.id.zeroize();
        // self.payload.zeroize();
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

    pub fn new_from_mnemonic(mnemonic: &str) -> Self {
        // TODO - check that mnemonic is valid
        Self {
            id: PrvKeyDataId::new_from_slice(&sha256_hash(mnemonic.as_bytes()).unwrap().as_ref()[0..8]),
            payload: Encryptable::Plain(KeyDataPayload::new(mnemonic.to_string())),
        }
    }

    pub fn encrypt(&mut self, secret: Secret) -> Result<()> {
        self.payload = self.payload.into_encrypted(secret)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PubKeyData {
    pub id: PubKeyDataId,
    pub keys: Vec<String>,
    pub cosigner_index: Option<u32>,
    pub minimum_signatures: Option<u16>,
}

impl Drop for PubKeyData {
    fn drop(&mut self) {
        self.keys.zeroize();
    }
}

impl PubKeyData {
    pub fn new(keys: Vec<String>, cosigner_index: Option<u32>, minimum_signatures: Option<u16>) -> Self {
        let mut temp = keys.clone();
        temp.sort();
        let str = String::from_iter(temp);
        let id = PubKeyDataId::new_from_slice(&sha256_hash(str.as_bytes()).unwrap().as_ref()[0..8]);
        Self { id, keys, cosigner_index, minimum_signatures }
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
    pub account_index: u64,
    pub is_visible: bool,
    pub pub_key_data: PubKeyData,
    pub prv_key_data_id: Option<PrvKeyDataId>,
    pub minimum_signatures: u16,
    pub cosigner_index: u32,
    pub ecdsa: bool,
}

impl Account {
    pub fn new(
        name: String,
        title: String,
        account_kind: AccountKind,
        account_index: u64,
        is_visible: bool,
        pub_key_data: PubKeyData,
        prv_key_data_id: Option<PrvKeyDataId>,
        ecdsa: bool,
        minimum_signatures: u16,
        cosigner_index: u32,
    ) -> Self {
        let id = pub_key_data.id.to_hex();
        Self {
            id,
            name,
            title,
            account_kind,
            account_index,
            pub_key_data,
            prv_key_data_id,
            is_visible,
            ecdsa,
            minimum_signatures,
            cosigner_index,
        }
    }
}

impl From<crate::account::Account> for Account {
    fn from(account: crate::account::Account) -> Self {
        let inner = account.inner();
        inner.stored.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub id: String,
    pub name: String,
    pub title: String,
    pub account_kind: AccountKind,
    pub pub_key_data: PubKeyData,
    pub ecdsa: bool,
    pub account_index: u64,
}

impl From<Account> for Metadata {
    fn from(account: Account) -> Self {
        Self {
            id: account.id,
            name: account.name,
            title: account.title,
            account_kind: account.account_kind,
            pub_key_data: account.pub_key_data,
            ecdsa: account.ecdsa,
            account_index: account.account_index,
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Payload {
    pub prv_key_data: Vec<PrvKeyData>,
    pub accounts: Vec<Account>,
}

impl Payload {
    pub fn add_prv_key_data(&mut self, mnemonic: Mnemonic, payment_secret: Option<Secret>) -> Result<PrvKeyData> {
        let key_data_payload = KeyDataPayload::new(mnemonic.phrase().to_string());
        let key_data_payload_id = key_data_payload.id();
        let key_data_payload = Encryptable::Plain(key_data_payload);

        let mut prv_key_data = PrvKeyData::new(key_data_payload_id, key_data_payload);
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret)?;
        }

        if !self.prv_key_data.iter().any(|existing_key_data| prv_key_data.id == existing_key_data.id) {
            self.prv_key_data.push(prv_key_data.clone());
        } else {
            panic!("private key data id already exists in the wallet");
        }

        Ok(prv_key_data)
    }
}

impl Zeroize for Payload {
    fn zeroize(&mut self) {
        self.prv_key_data.iter_mut().for_each(Zeroize::zeroize);
        // TODO
        // self.keydata.zeroize();
        // self.accounts.zeroize();
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct WalletSettings {
    pub account_id: String,
}
impl WalletSettings {
    pub fn new(account_id: String) -> Self {
        Self { account_id }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub settings: WalletSettings,
    pub payload: Encrypted,
    pub metadata: Vec<Metadata>,
}

impl Wallet {
    pub fn try_new(secret: Secret, settings: WalletSettings, payload: Payload) -> Result<Self> {
        let metadata = payload.accounts.iter().filter(|account| account.is_visible).map(|account| account.clone().into()).collect();
        let payload = Decrypted::new(payload).encrypt(secret)?;
        Ok(Self { settings, payload, metadata })
    }

    pub fn payload(&self, secret: Secret) -> Result<Decrypted<Payload>> {
        self.payload.decrypt::<Payload>(secret)
    }

    pub async fn try_load(store: &Store) -> Result<Wallet> {
        if fs::exists(store.filename()).await? {
            let wallet = fs::read_json::<Wallet>(store.filename()).await?;
            Ok(wallet)
        } else {
            Err(Error::NoWalletInStorage)
        }
    }

    pub async fn try_store(store: &Store, secret: Secret, settings: WalletSettings, payload: Payload) -> Result<()> {
        let wallet = Wallet::try_new(secret, settings, payload)?;
        store.ensure_dir().await?;
        fs::write_json(store.filename(), &wallet).await?;
        Ok(())
    }

    /// Obtain [`PrvKeyData`] by [`PrvKeyDataId`]
    pub async fn try_get_prv_key_data(&self, secret: Secret, prv_key_data_id: &PrvKeyDataId) -> Result<Option<PrvKeyData>> {
        let payload = self.payload.decrypt::<Payload>(secret)?;
        let idx = payload.as_ref().prv_key_data.iter().position(|keydata| &keydata.id == prv_key_data_id);
        let keydata = idx.map(|idx| payload.as_ref().prv_key_data.get(idx).unwrap().clone());
        Ok(keydata)
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
    pub fn new(filename: &str) -> Result<Store> {
        let filename = fs::resolve_path(filename);
        let filename = if runtime::is_web() {
            PathBuf::from(filename.file_name().ok_or(Error::InvalidFilename(format!("{}", filename.display())))?)
        } else {
            filename
        };

        Ok(Store { filename })
    }

    pub fn filename(&self) -> &PathBuf {
        &self.filename
    }

    pub async fn purge(&self) -> Result<()> {
        workflow_store::fs::remove(self.filename()).await?;
        Ok(())
    }

    pub async fn exists(&self) -> Result<bool> {
        Ok(workflow_store::fs::exists(self.filename()).await?)
    }

    pub async fn ensure_dir(&self) -> Result<()> {
        let file = self.filename();
        if file.exists() {
            return Ok(());
        }

        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).await?;
        }
        Ok(())
    }
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

        let store = Store::new("test-wallet-store")?;

        let mut payload = Payload::default();

        let global_password = Secret::from("ABC-L4LXw2F7HEK3wJU-Rk4stbPy6c");
        let password = Secret::from("test-123-# L4LXw2F7HEK3wJU Rk4stbPy6c");
        let mnemonic1 = "caution guide valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light nut invite fiction visa hamster coyote".to_string();
        let mnemonic2 = "nut invite fiction visa hamster coyote guide caution valley easily latin already visual fancy fork car switch runway vicious polar surprise fence boil light".to_string();

        let key_data_payload1 = KeyDataPayload::new(mnemonic1.clone());
        let prv_key_data1 = PrvKeyData::new(key_data_payload1.id(), Encryptable::Plain(key_data_payload1));

        let key_data_payload2 = KeyDataPayload::new(mnemonic2.clone());
        let prv_key_data2 =
            PrvKeyData::new(key_data_payload2.id(), Encryptable::Plain(key_data_payload2).into_encrypted(password.clone())?);

        let pub_key_data1 = PubKeyData::new(vec!["abc".to_string()], None, None);
        let pub_key_data2 = PubKeyData::new(vec!["xyz".to_string()], None, None);
        println!("keydata1 id: {:?}", prv_key_data1.id);
        //assert_eq!(prv_key_data.id.0, [79, 36, 5, 159, 220, 113, 179, 22]);
        payload.prv_key_data.push(prv_key_data1.clone());
        payload.prv_key_data.push(prv_key_data2.clone());

        let account1 = Account::new(
            "Wallet-A".to_string(),
            "Wallet A".to_string(),
            AccountKind::Bip32,
            0,
            true,
            pub_key_data1.clone(),
            Some(prv_key_data1.id),
            false,
            1,
            0,
        );
        let account_id = account1.id.clone();
        payload.accounts.push(account1);

        let account2 = Account::new(
            "Wallet-B".to_string(),
            "Wallet B".to_string(),
            AccountKind::Bip32,
            0,
            true,
            pub_key_data2.clone(),
            Some(prv_key_data2.id),
            false,
            1,
            0,
        );
        payload.accounts.push(account2);

        let payload_json = serde_json::to_string(&payload).unwrap();
        let settings = WalletSettings::new(account_id);
        Wallet::try_store(&store, global_password.clone(), settings, payload).await?;

        let w2 = Wallet::try_load(&store).await?;
        let w2payload = w2.payload.decrypt::<Payload>(global_password.clone()).unwrap();
        println!("\n---\nwallet.metadata (plain): {:#?}\n\n", w2.metadata);
        // let w2payload_json = serde_json::to_string(w2payload.as_ref()).unwrap();
        println!("\n---nwallet.payload (decrypted): {:#?}\n\n", w2payload.as_ref());
        // purge the store
        store.purge().await?;

        assert_eq!(payload_json, serde_json::to_string(w2payload.as_ref())?);

        let w2keydata1 = w2payload.as_ref().prv_key_data.get(0).unwrap();
        let w2keydata1_payload = w2keydata1.payload.decrypt(None).unwrap();
        let first_mnemonic = &w2keydata1_payload.as_ref().mnemonic;
        // println!("first mnemonic (plain): {}", hex_string(first_mnemonic.as_ref()));
        println!("first mnemonic (plain): {first_mnemonic}");
        assert_eq!(&mnemonic1, first_mnemonic);

        let w2keydata2 = w2payload.as_ref().prv_key_data.get(1).unwrap();
        let w2keydata2_payload = w2keydata2.payload.decrypt(Some(password.clone())).unwrap();
        let second_mnemonic = &w2keydata2_payload.as_ref().mnemonic;
        println!("second mnemonic (encrypted): {second_mnemonic}");
        assert_eq!(&mnemonic2, second_mnemonic);

        Ok(())
    }
}
