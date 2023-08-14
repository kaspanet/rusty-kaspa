#[allow(unused_imports)]
use crate::accounts::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
// use crate::address::{build_derivate_paths, AddressManager};
use crate::imports::*;
// use crate::result::Result;
// use crate::runtime::{AtomicBalance, Balance, BalanceStrings, Wallet};
// use crate::secret::Secret;
// use crate::storage::interface::AccessContext;
use crate::storage::{self, PrvKeyDataId};
// use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, KeydataSigner, PaymentDestination, PendingTransaction, Signer};
// use crate::utxo::{UtxoContext, UtxoContextBinding, UtxoEntryReference};
// use crate::AddressDerivationManager;
// use faster_hex::hex_string;
use kaspa_hashes::Hash;
use secp256k1::PublicKey;
// use futures::future::join_all;
// use kaspa_bip32::ChildNumber;
// use kaspa_notify::listener::ListenerId;
// use secp256k1::{ONE_KEY, PublicKey, SecretKey};
// use separator::Separatable;
// use serde::Serializer;
// use std::hash::Hash;
// use std::str::FromStr;
// use workflow_core::abortable::Abortable;
// use workflow_core::enums::u8_try_from;
// use kaspa_addresses::Version as AddressVersion;
// use crate::storage::AccountData as StorageData;
use crate::encryption::sha256_hash;
use crate::runtime::account::AccountKind;

// #[derive(Hash)]
#[derive(BorshSerialize)]
struct AccountIdHashData {
    account_kind: AccountKind,
    prv_key_data_id: Option<PrvKeyDataId>,
    ecdsa: Option<bool>,
    account_index: Option<u64>,
    secp256k1_public_key: Option<Vec<u8>>,
    data: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub(crate) Hash);

impl AccountId {
    pub(crate) fn from_bip32(prv_key_data_id: &PrvKeyDataId, data: &storage::account::Bip32) -> AccountId {
        let hashable = AccountIdHashData {
            account_kind: AccountKind::Bip32,
            prv_key_data_id: Some(prv_key_data_id.clone()),
            ecdsa: Some(data.ecdsa),
            account_index: Some(data.account_index),
            secp256k1_public_key: None,
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub(crate) fn from_legacy(prv_key_data_id: &PrvKeyDataId, _data: &storage::account::Legacy) -> AccountId {
        let hashable = AccountIdHashData {
            account_kind: AccountKind::Legacy,
            prv_key_data_id: Some(prv_key_data_id.clone()),
            ecdsa: Some(false),
            account_index: Some(0),
            secp256k1_public_key: None,
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub(crate) fn from_multisig(prv_key_data_id: &PrvKeyDataId, data: &storage::account::MultiSig) -> AccountId {
        let hashable = AccountIdHashData {
            account_kind: AccountKind::MultiSig,
            prv_key_data_id: Some(prv_key_data_id.clone()),
            ecdsa: Some(data.ecdsa),
            account_index: Some(0),
            secp256k1_public_key: None,
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub(crate) fn from_keypair(prv_key_data_id: &PrvKeyDataId, data: &storage::account::Keypair) -> AccountId {
        let hashable = AccountIdHashData {
            account_kind: AccountKind::Keypair,
            prv_key_data_id: Some(prv_key_data_id.clone()),
            ecdsa: Some(data.ecdsa),
            account_index: None,
            secp256k1_public_key: Some(data.public_key.serialize().to_vec()),
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    // pub(crate) fn from_storage_data(data : &storage::AccountData) -> AccountId {

    //     let hashable = match data {
    //         storage::AccountData::Legacy { prv_key_data_id,  .. } => {
    //             AccountIdHashData {
    //                 account_kind: AccountKind::Legacy,
    //                 prv_key_data_id: Some(*prv_key_data_id), ecdsa : Some(false),
    //                 account_index: Some(0),
    //                 secp256k1_public_key: None,
    //                 resident_data : None,
    //             }
    //         },
    //         storage::AccountData::Bip32 { prv_key_data_id, ecdsa, account_index, .. } => {
    //             AccountIdHashData {
    //                 account_kind: AccountKind::Bip32,
    //                 prv_key_data_id: Some(*prv_key_data_id),
    //                 ecdsa: Some(*ecdsa),
    //                 account_index: Some(*account_index),
    //                 secp256k1_public_key: None,
    //                 resident_data : None,
    //             }
    //         },
    //         storage::AccountData::MultiSig { prv_key_data_id, ecdsa, account_index, .. } => {
    //             AccountIdHashData {
    //                 account_kind: AccountKind::MultiSig,
    //                 prv_key_data_id: Some(*prv_key_data_id),
    //                 ecdsa: Some(*ecdsa),
    //                 account_index: Some(*account_index),
    //                 secp256k1_public_key : None,
    //                 resident_data : None,
    //             }
    //         },
    //         storage::AccountData::Secp256k1Keypair { prv_key_data_id, public_key, ecdsa } => {
    //             AccountIdHashData {
    //                 account_kind: AccountKind::Secp256k1Keypair,
    //                 prv_key_data_id: None,
    //                 ecdsa: Some(*ecdsa),
    //                 account_index: None,
    //                 secp256k1_public_key: Some(public_key.serialize().to_vec()),
    //                 resident_data : None,
    //             }
    //         },
    //         // storage::AccountData::Resident { prv_key_data_id, ecdsa, account_index, .. } => {
    //         //     AccountIdHashData { prv_key_data_id: *prv_key_data_id, ecdsa: *ecdsa,
    //         //         account_kind: AccountKind::Resident, account_index: *account_index,
    //         //     secp256k1_public_key: None }
    //         // },
    //     };

    //     // let data = AccountIdHashData { prv_key_data_id: *prv_key_data_id, ecdsa, account_kind: *account_kind, account_index };
    //     let hash = sha256_hash(hashable.try_to_vec().unwrap().as_slice());
    //     AccountId(Hash::from_slice(hash.as_ref()))
    //     // AccountId(xxh3_64(hashable.try_to_vec().unwrap().as_slice()))
    // }

    pub fn from_public_key(account_kind: AccountKind, public_key: &PublicKey) -> Self {
        let hashable = AccountIdHashData {
            account_kind,
            prv_key_data_id: None,
            ecdsa: None,
            account_index: None,
            secp256k1_public_key: Some(public_key.serialize().to_vec()),
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub fn from_data(account_kind: AccountKind, data: &[u8]) -> Self {
        let hashable = AccountIdHashData {
            account_kind,
            prv_key_data_id: None,
            ecdsa: None,
            account_index: None,
            secp256k1_public_key: None,
            data: Some(data.to_vec()),
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }
    // pub(crate) fn new(prv_key_data_id: &PrvKeyDataId, ecdsa: bool, account_kind: &AccountKind, account_index: u64) -> AccountId {
    //     let data = AccountIdHashData { prv_key_data_id: *prv_key_data_id, ecdsa, account_kind: *account_kind, account_index };
    //     AccountId(xxh3_64(data.try_to_vec().unwrap().as_slice()))
    // }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("[{}]", &hex[0..4])
    }
}

impl ToHex for AccountId {
    fn to_hex(&self) -> String {
        format!("{}", self.0)
    }
}

// impl Serialize for AccountId {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         // self.0.serialize(&mut serializer)
//         // serializer.serialize(&self.0)
//         // serializer.serialize_str(&hex_string(&self.0.to_be_bytes()))
//     }
// }

// impl<'de> Deserialize<'de> for AccountId {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {

//         let hex_str = <std::string::String as Deserialize>::deserialize(deserializer)?;
//         let mut out = [0u8; 8];
//         let mut input = [b'0'; 16];
//         let start = input.len() - hex_str.len();
//         input[start..].copy_from_slice(hex_str.as_bytes());
//         faster_hex::hex_decode(&input, &mut out).map_err(serde::de::Error::custom)?;
//         Ok(AccountId(u64::from_be_bytes(out)))
//     }
// }

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // write!(f, "{}", hex_string(&self.0.to_be_bytes()))
        write!(f, "{}", self.0)
    }
}
