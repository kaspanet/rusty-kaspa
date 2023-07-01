use crate::imports::*;
use crate::storage::{AccountId, AccountKind, PrvKeyDataId, PubKeyData};
use zeroize::Zeroize;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub title: String,
    pub account_kind: AccountKind,
    pub account_index: u64,
    pub is_visible: bool,
    pub pub_key_data: PubKeyData,
    pub prv_key_data_id: PrvKeyDataId,
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
        prv_key_data_id: PrvKeyDataId,
        ecdsa: bool,
        minimum_signatures: u16,
        cosigner_index: u32,
    ) -> Self {
        Self {
            id: AccountId::new(&prv_key_data_id, ecdsa, &account_kind, account_index),
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

impl From<crate::runtime::Account> for Account {
    fn from(account: crate::runtime::Account) -> Self {
        let inner = account.inner();
        inner.stored.clone()
    }
}

impl Zeroize for Account {
    fn zeroize(&mut self) {
        self.prv_key_data_id.zeroize();
        // TODO
    }
}
