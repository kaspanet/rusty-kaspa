// use crate::imports::*;
// use crate::storage::{Account, AccountId, AccountKind, PubKeyData};
use crate::storage::Account;

pub type Metadata = Account;

// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct Metadata {
//     pub id: AccountId,
//     pub name: String,
//     pub title: String,
//     pub account_kind: AccountKind,
//     pub pub_key_data: PubKeyData,
//     pub ecdsa: bool,
//     pub account_index: u64,
// }

// impl From<Account> for Metadata {
//     fn from(account: Account) -> Self {
//         Self {
//             id: account.id,
//             name: account.name,
//             title: account.title,
//             account_kind: account.account_kind,
//             pub_key_data: account.pub_key_data,
//             ecdsa: account.ecdsa,
//             account_index: account.account_index,
//         }
//     }
// }
