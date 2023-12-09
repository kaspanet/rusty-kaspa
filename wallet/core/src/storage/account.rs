use crate::imports::*;
use crate::storage::{AccountId, AccountKind, PrvKeyDataId};
use kaspa_hashes::Hash;
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
    pub version: u16,
}

impl Legacy {
    pub fn new() -> Self {
        Self { version: LEGACY_ACCOUNT_VERSION }
    }
}

impl Default for Legacy {
    fn default() -> Self {
        Self::new()
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

const HTLC_ACCOUNT_VERSION: u16 = 0;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum HtlcRole {
    Sender = 0,
    Receiver = 1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct HTLC {
    #[serde(default)]
    pub version: u16,
    pub xpub_key: Arc<String>,
    pub second_party_address: Arc<Address>,
    pub account_index: u64,
    pub ecdsa: bool,
    pub role: HtlcRole,
    pub locktime: u64,
    pub secret_hash: Hash,
}

impl HTLC {
    pub fn new(
        xpub_key: Arc<String>,
        second_party_xpub_key: Arc<Address>,
        account_index: u64,
        ecdsa: bool,
        role: HtlcRole,
        locktime: u64,
        secret_hash: Hash,
    ) -> Self {
        Self {
            version: HTLC_ACCOUNT_VERSION,
            xpub_key,
            second_party_address: second_party_xpub_key,
            account_index,
            ecdsa,
            role,
            locktime,
            secret_hash,
        }
    }
}

const KEYPAIR_ACCOUNT_VERSION: u16 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Keypair {
    #[serde(default)]
    pub version: u16,

    pub public_key: String,
    pub ecdsa: bool,
}

impl Keypair {
    pub fn new(public_key: PublicKey, ecdsa: bool) -> Self {
        Self { version: KEYPAIR_ACCOUNT_VERSION, public_key: public_key.to_string(), ecdsa }
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
    Htlc(HTLC),
}

impl AccountData {
    pub fn account_kind(&self) -> AccountKind {
        match self {
            AccountData::Legacy { .. } => AccountKind::Legacy,
            AccountData::Bip32 { .. } => AccountKind::Bip32,
            AccountData::MultiSig { .. } => AccountKind::MultiSig,
            AccountData::Hardware { .. } => AccountKind::Hardware,
            AccountData::Keypair { .. } => AccountKind::Keypair,
            AccountData::Htlc(_) => AccountKind::HTLC,
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

    pub fn is_legacy(&self) -> bool {
        matches!(self.data, AccountData::Legacy { .. })
    }
}
