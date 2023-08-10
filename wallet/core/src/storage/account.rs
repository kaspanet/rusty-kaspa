use crate::imports::*;
use crate::storage::{AccountId, AccountKind, PrvKeyDataId, PubKeyData};
// use crate::storage::{AccountId, PrvKeyDataId};
use secp256k1::PublicKey;
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum AccountData {
    Legacy {
        xpub_keys: Vec<String>,
        prv_key_data_id: PrvKeyDataId,
    },
    Bip32 {
        xpub_keys: Vec<String>,
        cosigner_index: u8,
        minimum_signatures: u16,
        prv_key_data_id: PrvKeyDataId,
        ecdsa: bool,    
    },
    MultiSig {
        xpub_keys: Vec<String>,
        prv_key_data_id: PrvKeyDataId,
        cosigner_index: u8,
        minimum_signatures: u16,
        ecdsa: bool,    
    },
    Secp256k1Keypair {
        public_key: PublicKey,
        prv_key_data_id: PrvKeyDataId,
    },
    /// Account that is not stored in the database
    /// and is only used for signing transactions
    /// during the lifecycle of the runtime.
    /// Used by Rust and JavaScript runtime APIs.
    Resident,
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub is_visible: bool,
    pub data : AccountData,
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
        name: Option<String>,
        title: Option<String>,
        is_visible: bool,
        data : AccountData,
        // account_kind: AccountKind,
        // account_index: u64,
        // pub_key_data: PubKeyData,
        // prv_key_data_id: PrvKeyDataId,
        // ecdsa: bool,
        // minimum_signatures: u16,
        // cosigner_index: u32,
    ) -> Self {
        Self {
            id: AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index),
            name,
            title,
            is_visible,
            data
            // account_kind,
            // account_index,
            // pub_key_data,
            // prv_key_data_id,
            // ecdsa,
            // minimum_signatures,
            // cosigner_index,
        }
    }
}

impl From<crate::runtime::Account> for Account {
    fn from(account: crate::runtime::Account) -> Self {
        let inner = account.inner();
        inner.stored.clone()
    }
}

impl Zeroize for Account {
    fn zeroize(&mut self) {
        self.prv_key_data_id.zeroize();
    }
}
