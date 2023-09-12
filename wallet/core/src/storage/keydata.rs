use crate::derivation::create_xpub_from_xprv;
use crate::imports::*;
use crate::result::Result;
use crate::secret::Secret;
use faster_hex::{hex_decode, hex_string};
use kaspa_bip32::{ExtendedPrivateKey, ExtendedPublicKey, Language, Mnemonic};
use secp256k1::SecretKey;
use serde::Serializer;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
#[allow(unused_imports)]
use workflow_core::runtime;
use xxhash_rust::xxh3::xxh3_64;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::storage::{AccountKind, Encryptable};

#[derive(Default, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, Ord, PartialOrd)]
pub struct KeyDataId(pub(crate) [u8; 8]);

impl KeyDataId {
    pub fn new(id: u64) -> Self {
        KeyDataId(id.to_le_bytes())
    }

    pub fn new_from_slice(vec: &[u8]) -> Self {
        Self(<[u8; 8]>::try_from(<&[u8]>::clone(&vec)).expect("Error: invalid slice size for id"))
    }
}

impl ToHex for KeyDataId {
    fn to_hex(&self) -> String {
        self.0.to_vec().to_hex()
    }
}

impl FromHex for KeyDataId {
    type Error = Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        let mut data = vec![0u8; hex_str.len() / 2];
        hex_decode(hex_str.as_bytes(), &mut data)?;
        Ok(Self::new_from_slice(&data))
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
pub type PrvKeyDataMap = HashMap<PrvKeyDataId, PrvKeyData>;

/// Indicates key capabilities in the context of Kaspa
/// core (kaspa-wallet) or legacy (KDX/PWA) wallets.
/// The setting is based on the type of key import.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyCaps {
    // 24 word mnemonic, bip39 seed accounts
    MultipleAccounts,
    // 12 word mnemonic (legacy)
    SingleAccount,
}

impl KeyCaps {
    pub fn from_mnemonic_phrase(phrase: &str) -> Self {
        let data = phrase.split_whitespace().collect::<Vec<_>>();
        if data.len() == 12 {
            KeyCaps::SingleAccount
        } else {
            KeyCaps::MultipleAccounts
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "key-variant", content = "key-data")]
pub enum PrvKeyVariant {
    // 12 (legacy) or 24 word Bip39 mnemonic
    Mnemonic(String),
    // Bip39 seed (generated from mnemonic)
    Bip39Seed(String),
    // Extended Private Key (XPrv)
    ExtendedPrivateKey(String),
    // SecretKey
    SecretKey(String),
}

impl PrvKeyVariant {
    pub fn from_mnemonic(mnemonic: Mnemonic) -> Self {
        PrvKeyVariant::Mnemonic(mnemonic.phrase_string())
    }

    pub fn from_secret_key(secret_key: SecretKey) -> Self {
        PrvKeyVariant::SecretKey(secret_key.secret_bytes().to_vec().to_hex())
    }

    pub fn get_string(&self) -> Zeroizing<String> {
        match self {
            PrvKeyVariant::Mnemonic(s) => Zeroizing::new(s.clone()),
            PrvKeyVariant::Bip39Seed(s) => Zeroizing::new(s.clone()),
            PrvKeyVariant::ExtendedPrivateKey(s) => Zeroizing::new(s.clone()),
            PrvKeyVariant::SecretKey(s) => Zeroizing::new(s.clone()),
        }
    }

    pub fn id(&self) -> KeyDataId {
        let s = self.get_string();
        PrvKeyDataId::new(xxh3_64(s.as_bytes()))
    }

    // pub fn get_caps(&self) -> KeyCaps {
    //     match self {
    //         PrvKeyVariant::Mnemonic(phrase) => { KeyCaps::from_mnemonic_phrase(phrase) },
    //         PrvKeyVariant::Bip39Seed(_) => { KeyCaps::MultipleAccounts },
    //         PrvKeyVariant::ExtendedPrivateKey(_) => { KeyCaps::SingleAccount },
    //     }
    // }

    // pub fn get_as_bytes(&self) -> Zeroizing<&[u8]> {
    //     match self {
    //         PrvKeyVariant::Mnemonic(s) => Zeroizing::new(s.clone()),
    //         PrvKeyVariant::ExtendedPrivateKey(s) => Zeroizing::new(s.clone()),
    //         PrvKeyVariant::Seed(s) => Zeroizing::new(s.clone()),
    //     }
    // }
}

impl Zeroize for PrvKeyVariant {
    fn zeroize(&mut self) {
        match self {
            PrvKeyVariant::Mnemonic(s) => s.zeroize(),
            PrvKeyVariant::Bip39Seed(s) => s.zeroize(),
            PrvKeyVariant::ExtendedPrivateKey(s) => s.zeroize(),
            PrvKeyVariant::SecretKey(s) => s.zeroize(),
        }
    }
}
impl Drop for PrvKeyVariant {
    fn drop(&mut self) {
        self.zeroize()
    }
}

impl ZeroizeOnDrop for PrvKeyVariant {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyDataPayload {
    prv_key_variant: PrvKeyVariant,
}

impl PrvKeyDataPayload {
    pub fn try_new_with_mnemonic(mnemonic: Mnemonic) -> Result<Self> {
        Ok(Self { prv_key_variant: PrvKeyVariant::from_mnemonic(mnemonic) })
    }

    pub fn try_new_with_secret_key(secret_key: SecretKey) -> Result<Self> {
        Ok(Self { prv_key_variant: PrvKeyVariant::from_secret_key(secret_key) })
    }

    pub fn get_xprv(&self, payment_secret: Option<&Secret>) -> Result<ExtendedPrivateKey<SecretKey>> {
        let payment_secret = payment_secret.map(|s| std::str::from_utf8(s.as_ref())).transpose()?;

        match &self.prv_key_variant {
            PrvKeyVariant::Mnemonic(mnemonic) => {
                let mnemonic = Mnemonic::new(mnemonic, Language::English)?;
                let xkey = ExtendedPrivateKey::<SecretKey>::new(mnemonic.to_seed(payment_secret.unwrap_or_default()))?;
                Ok(xkey)
            }
            PrvKeyVariant::Bip39Seed(seed) => {
                let seed = Zeroizing::new(Vec::from_hex(seed.as_ref())?);
                let xkey = ExtendedPrivateKey::<SecretKey>::new(seed)?;
                Ok(xkey)
            }
            PrvKeyVariant::ExtendedPrivateKey(extended_private_key) => {
                let xkey: ExtendedPrivateKey<SecretKey> = extended_private_key.parse()?;
                Ok(xkey)
            }
            PrvKeyVariant::SecretKey(_) => Err(Error::XPrvSupport),
        }
    }

    pub fn as_mnemonic(&self) -> Result<Option<Mnemonic>> {
        match &self.prv_key_variant {
            PrvKeyVariant::Mnemonic(mnemonic) => Ok(Some(Mnemonic::new(mnemonic.clone(), Language::English)?)),
            _ => Ok(None),
        }
    }

    pub fn as_variant(&self) -> Zeroizing<PrvKeyVariant> {
        Zeroizing::new(self.prv_key_variant.clone())
    }

    pub fn as_secret_key(&self) -> Result<Option<SecretKey>> {
        match &self.prv_key_variant {
            PrvKeyVariant::SecretKey(private_key) => Ok(Some(SecretKey::from_str(private_key)?)),
            _ => Ok(None),
        }
    }

    pub fn id(&self) -> PrvKeyDataId {
        self.prv_key_variant.id()
    }
}

impl Zeroize for PrvKeyDataPayload {
    fn zeroize(&mut self) {
        self.prv_key_variant.zeroize();
    }
}

impl Drop for PrvKeyDataPayload {
    fn drop(&mut self) {
        self.zeroize()
    }
}

impl ZeroizeOnDrop for PrvKeyDataPayload {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrvKeyData {
    pub id: PrvKeyDataId,
    pub name: Option<String>,
    pub key_caps: KeyCaps,
    pub payload: Encryptable<PrvKeyDataPayload>,
}

impl PrvKeyData {
    pub async fn create_xpub(
        &self,
        payment_secret: Option<&Secret>,
        account_kind: AccountKind,
        account_index: u64,
    ) -> Result<ExtendedPublicKey<secp256k1::PublicKey>> {
        let payload = self.payload.decrypt(payment_secret)?;
        let xprv = payload.get_xprv(payment_secret)?;
        create_xpub_from_xprv(xprv, account_kind, account_index).await
    }

    pub fn as_mnemonic(&self, payment_secret: Option<&Secret>) -> Result<Option<Mnemonic>> {
        let payload = self.payload.decrypt(payment_secret)?;
        payload.as_mnemonic()
    }

    pub fn as_variant(&self, payment_secret: Option<&Secret>) -> Result<Zeroizing<PrvKeyVariant>> {
        let payload = self.payload.decrypt(payment_secret)?;
        Ok(payload.as_variant())
    }
}

impl TryFrom<(Mnemonic, Option<&Secret>)> for PrvKeyData {
    type Error = Error;
    fn try_from((mnemonic, payment_secret): (Mnemonic, Option<&Secret>)) -> Result<Self> {
        let account_caps = KeyCaps::from_mnemonic_phrase(mnemonic.phrase());
        let key_data_payload = PrvKeyDataPayload::try_new_with_mnemonic(mnemonic)?;
        let key_data_payload_id = key_data_payload.id();
        let key_data_payload = Encryptable::Plain(key_data_payload);

        let mut prv_key_data = PrvKeyData::new(key_data_payload_id, None, account_caps, key_data_payload);
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret)?;
        }

        Ok(prv_key_data)
    }
}

impl Zeroize for PrvKeyData {
    fn zeroize(&mut self) {
        self.id.zeroize();
        self.name.zeroize();
        self.payload.zeroize();
    }
}

impl Drop for PrvKeyData {
    fn drop(&mut self) {
        self.zeroize();
    }
}

impl PrvKeyData {
    pub fn new(id: PrvKeyDataId, name: Option<String>, key_caps: KeyCaps, payload: Encryptable<PrvKeyDataPayload>) -> Self {
        Self { id, payload, key_caps, name }
    }

    pub fn try_new_from_mnemonic(mnemonic: Mnemonic, payment_secret: Option<&Secret>) -> Result<Self> {
        let key_caps = KeyCaps::from_mnemonic_phrase(mnemonic.phrase());
        let payload = PrvKeyDataPayload::try_new_with_mnemonic(mnemonic)?;
        let mut prv_key_data = Self { id: payload.id(), payload: Encryptable::Plain(payload), key_caps, name: None };
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret)?;
        }

        Ok(prv_key_data)
    }

    pub fn try_new_from_secret_key(secret_key: SecretKey, payment_secret: Option<&Secret>) -> Result<Self> {
        let key_caps = KeyCaps::SingleAccount;
        let payload = PrvKeyDataPayload::try_new_with_secret_key(secret_key)?;
        let mut prv_key_data = Self { id: payload.id(), payload: Encryptable::Plain(payload), key_caps, name: None };
        if let Some(payment_secret) = payment_secret {
            prv_key_data.encrypt(payment_secret)?;
        }

        Ok(prv_key_data)
    }

    pub fn encrypt(&mut self, secret: &Secret) -> Result<()> {
        self.payload = self.payload.into_encrypted(secret)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct PrvKeyDataInfo {
    pub id: PrvKeyDataId,
    pub name: Option<String>,
    pub key_caps: KeyCaps,
    pub is_encrypted: bool,
}

impl From<&PrvKeyData> for PrvKeyDataInfo {
    fn from(data: &PrvKeyData) -> Self {
        Self::new(data.id, data.name.clone(), data.key_caps.clone(), data.payload.is_encrypted())
    }
}

impl PrvKeyDataInfo {
    pub fn new(id: PrvKeyDataId, name: Option<String>, key_caps: KeyCaps, is_encrypted: bool) -> Self {
        Self { id, name, key_caps, is_encrypted }
    }

    pub fn is_encrypted(&self) -> bool {
        self.is_encrypted
    }
}

impl Display for PrvKeyDataInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(name) = &self.name {
            write!(f, "{} ({})", name, self.id.to_hex())?;
        } else {
            write!(f, "{}", self.id.to_hex())?;
        }
        Ok(())
    }
}
