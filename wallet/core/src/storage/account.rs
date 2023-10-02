use crate::imports::*;
use crate::storage::{AccountId, AccountKind, PrvKeyDataId};
use secp256k1::PublicKey;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Settings {
    pub is_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

const LEGACY_ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Legacy {
    #[serde(default)]
    pub version: u16,

    pub xpub_keys: Arc<Vec<String>>,
}

impl Legacy {
    pub fn new(xpub_keys: Arc<Vec<String>>) -> Self {
        Self { version: LEGACY_ACCOUNT_VERSION, xpub_keys }
    }
}

const BIP32_ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Bip32 {
    #[serde(default)]
    pub version: u16,

    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub ecdsa: bool,
}

impl Bip32 {
    pub fn new(account_index: u64, xpub_keys: Arc<Vec<String>>, ecdsa: bool) -> Self {
        Self { version: BIP32_ACCOUNT_VERSION, account_index, xpub_keys, ecdsa }
    }
}

const MULTISIG_ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct MultiSig {
    #[serde(default)]
    pub version: u16,
    pub xpub_keys: Arc<Vec<String>>,
    pub prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
    pub cosigner_index: Option<u8>,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

impl MultiSig {
    pub fn new(
        xpub_keys: Arc<Vec<String>>,
        prv_key_data_ids: Option<Arc<Vec<PrvKeyDataId>>>,
        cosigner_index: Option<u8>,
        minimum_signatures: u16,
        ecdsa: bool,
    ) -> Self {
        Self { version: MULTISIG_ACCOUNT_VERSION, xpub_keys, prv_key_data_ids, cosigner_index, minimum_signatures, ecdsa }
    }
}

const KEYPAIR_ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Keypair {
    #[serde(default)]
    pub version: u16,

    pub public_key: PublicKey,
    pub ecdsa: bool,
}

impl Keypair {
    pub fn new(public_key: PublicKey, ecdsa: bool) -> Self {
        Self { version: KEYPAIR_ACCOUNT_VERSION, public_key, ecdsa }
    }
}

const HARDWARE_ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Hardware {
    #[serde(default)]
    pub version: u16,

    pub descriptor: String,
}

impl Hardware {
    pub fn new(descriptor: &str) -> Self {
        Self { version: HARDWARE_ACCOUNT_VERSION, descriptor: descriptor.to_string() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum AccountData {
    Legacy(Legacy),
    Bip32(Bip32),
    MultiSig(MultiSig),
    Keypair(Keypair),
    Hardware(Hardware),
}

impl AccountData {
    pub fn account_kind(&self) -> AccountKind {
        match self {
            AccountData::Legacy { .. } => AccountKind::Legacy,
            AccountData::Bip32 { .. } => AccountKind::Bip32,
            AccountData::MultiSig { .. } => AccountKind::MultiSig,
            AccountData::Hardware { .. } => AccountKind::Hardware,
            AccountData::Keypair { .. } => AccountKind::Keypair,
        }
    }
}

const ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    #[serde(default)]
    pub version: u16,

    pub id: AccountId,
    pub prv_key_data_id: Option<PrvKeyDataId>,
    pub settings: Settings,
    pub data: AccountData,
}

impl Account {
    pub fn new(id: AccountId, prv_key_data_id: Option<PrvKeyDataId>, settings: Settings, data: AccountData) -> Self {
        Self { version: ACCOUNT_VERSION, id, prv_key_data_id, settings, data }
    }

    pub fn data(&self) -> &AccountData {
        &self.data
    }
}
