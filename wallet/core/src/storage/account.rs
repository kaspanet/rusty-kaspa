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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Legacy {
    pub xpub_keys: Arc<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Bip32 {
    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub ecdsa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct MultiSig {
    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub cosigner_index: u8,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Keypair {
    pub public_key: PublicKey,
    pub ecdsa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Hardware {}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub prv_key_data_id: Option<PrvKeyDataId>,
    pub settings: Settings,
    pub data: AccountData,
}

impl Account {
    pub fn new(id: AccountId, prv_key_data_id: Option<PrvKeyDataId>, settings: Settings, data: AccountData) -> Self {
        Self { id, prv_key_data_id, settings, data }
    }

    pub fn data(&self) -> &AccountData {
        &self.data
    }
}

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct AccountV0_0_0 {
//     pub id: AccountId,
//     pub prv_key_data_id: PrvKeyDataId,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub name: Option<String>,
//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub title: Option<String>,
//     pub is_visible: bool,
//     pub account_kind: AccountKind,
//     pub account_index: u64,
//     pub pub_key_data: PubKeyData,
//     pub prv_key_data_id: PrvKeyDataId,
//     pub minimum_signatures: u16,
//     pub cosigner_index: u32,
//     pub ecdsa: bool,
// }
