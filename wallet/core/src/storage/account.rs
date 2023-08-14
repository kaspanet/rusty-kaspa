use crate::imports::*;
// use crate::storage::{AccountId, AccountKind, PrvKeyDataId, PubKeyData};
use crate::storage::{AccountId, AccountKind, PrvKeyDataId};
// use crate::storage::{AccountId, PrvKeyDataId};
use secp256k1::PublicKey;
// use zeroize::Zeroize;

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
    pub prv_key_data_id: PrvKeyDataId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Bip32 {
    // pub prv_key_data_id: PrvKeyDataId,
    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub ecdsa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct MultiSig {
    // pub prv_key_data_id: PrvKeyDataId,
    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub cosigner_index: u8,
    pub minimum_signatures: u16,
    pub ecdsa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Keypair {
    // pub prv_key_data_id: PrvKeyDataId,
    pub public_key: PublicKey,
    pub ecdsa: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum AccountData {
    Legacy(Legacy),
    Bip32(Bip32),
    MultiSig(MultiSig),
    Keypair(Keypair), // Legacy {
                      //     prv_key_data_id: PrvKeyDataId,
                      //     xpub_keys: Arc<Vec<String>>,
                      // },
                      // Bip32 {
                      //     prv_key_data_id: PrvKeyDataId,
                      //     account_index : u64,
                      //     xpub_keys: Arc<Vec<String>>,
                      //     ecdsa: bool,
                      // },
                      // MultiSig {
                      //     prv_key_data_id: PrvKeyDataId,
                      //     account_index : u64,
                      //     xpub_keys: Arc<Vec<String>>,
                      //     cosigner_index: u8,
                      //     minimum_signatures: u16,
                      //     ecdsa: bool,
                      // },
                      // Secp256k1Keypair {
                      //     prv_key_data_id: PrvKeyDataId,
                      //     public_key: PublicKey,
                      //     ecdsa : bool,
                      // },
}

impl AccountData {
    pub fn account_kind(&self) -> AccountKind {
        match self {
            AccountData::Legacy { .. } => AccountKind::Legacy,
            AccountData::Bip32 { .. } => AccountKind::Bip32,
            AccountData::MultiSig { .. } => AccountKind::MultiSig,
            AccountData::Keypair { .. } => AccountKind::Keypair,
            // AccountData::Resident => AccountKind::Resident,
        }
    }

    //     pub fn id(&self) -> AccountId {

    //         match self {
    //             AccountData::Legacy {
    //                 prv_key_data_id, ..
    //             } => AccountId::new(&prv_key_data_id, false, &AccountKind::Legacy, 0),
    //             AccountData::Bip32 {
    //                 prv_key_data_id, account_index, ..
    //             } => AccountId::new(&prv_key_data_id, false, &AccountKind::Bip32, *account_index),
    //             AccountData::MultiSig {
    //                 prv_key_data_id, account_index, ..
    //             } => AccountId::new(&prv_key_data_id, false, &AccountKind::MultiSig, *account_index),
    //             AccountData::Secp256k1Keypair {
    //                 prv_key_data_id, ..
    //             } => AccountId::new(&prv_key_data_id, true, &AccountKind::Secp256k1Keypair, 0),
    //             AccountData::Resident => {
    //                 panic!("resident accounts are not allowed in storage")
    // //                AccountId::new(&PrvKeyDataId::from(&PublicKey::from_slice(&[0; 33]).unwrap()), true, &AccountKind::Resident, 0)
    //             },
    //         }

    //     }
}

// impl Zeroize for AccountData {
//     fn zeroize(&mut self) {
//         // self.data.zeroize();
//     }
// }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub prv_key_data_id: PrvKeyDataId,

    pub settings: Settings,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub name: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub title: Option<String>,
    // pub is_visible: bool,
    pub data: AccountData,
    // pub account_kind: AccountKind,
    // pub account_index: u64,
    // pub pub_key_data: PubKeyData,
    // pub prv_key_data_id: PrvKeyDataId,
    // pub minimum_signatures: u16,
    // pub cosigner_index: u32,
    // pub ecdsa: bool,
}

impl Account {
    pub fn new(
        id: AccountId,
        prv_key_data_id: PrvKeyDataId,
        settings: Settings,
        // name: Option<String>,
        // title: Option<String>,
        // is_visible: bool,
        data: AccountData,
        // account_kind: AccountKind,
        // account_index: u64,
        // pub_key_data: PubKeyData,
        // prv_key_data_id: PrvKeyDataId,
        // ecdsa: bool,
        // minimum_signatures: u16,
        // cosigner_index: u32,
    ) -> Self {
        Self {
            id, //: data.id(), //AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index),
            prv_key_data_id,
            // name,
            // title,
            // is_visible,
            settings,
            data,
            // account_kind,
            // account_index,
            // pub_key_data,
            // prv_key_data_id,
            // ecdsa,
            // minimum_signatures,
            // cosigner_index,
        }
    }

    pub fn data(&self) -> &AccountData {
        &self.data
    }
}

// impl From<crate::runtime::Account> for Account {
//     fn from(account: crate::runtime::Account) -> Self {
//         account.context().stored.expect("attempt to obtain storage::Account from a resident runtime::Account").clone()
//     }
// }

// impl Zeroize for Account {
//     fn zeroize(&mut self) {
//         self.data.zeroize();
//     }
// }
