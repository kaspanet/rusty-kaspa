use std::collections::HashMap;

// use crate::account::AccountId;
use crate::address::create_xpub_from_mnemonic;
use crate::result::Result;
use crate::secret::Secret;
use crate::{encryption::sha256_hash, imports::*};
use faster_hex::{hex_decode, hex_string};
use kaspa_bip32::{ExtendedPublicKey, Language, Mnemonic};
use serde::Serializer;
#[allow(unused_imports)]
use workflow_core::runtime;
use zeroize::Zeroize;

use crate::storage::{AccountKind, Encryptable};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
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
pub type PrvKeyDataMap = HashMap<PrvKeyDataId, PrvKeyData>;

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

    pub fn as_mnemonic(&self, payment_secret: Option<Secret>) -> Result<Mnemonic> {
        let payload = self.payload.decrypt(payment_secret)?;
        let words = payload.as_ref().mnemonic.as_str();
        // Mnemonic::new(phrase, language)
        let mnemonic = Mnemonic::new(words, Language::English)?;
        Ok(mnemonic)
        // Ok(())
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

#[derive(Clone)]
pub struct PrvKeyDataInfo {
    pub id: PrvKeyDataId,
    pub is_encrypted: bool,
}

impl From<&PrvKeyData> for PrvKeyDataInfo {
    fn from(data: &PrvKeyData) -> Self {
        Self::new(data.id, data.payload.is_encrypted())
    }
}

impl PrvKeyDataInfo {
    pub fn new(id: PrvKeyDataId, is_encrypted: bool) -> Self {
        Self { id, is_encrypted }
    }

    pub fn is_encrypted(&self) -> bool {
        self.is_encrypted
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
