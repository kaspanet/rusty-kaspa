#[allow(unused_imports)]
use crate::derivation::{gen0::*, gen1::*, PubkeyDerivationManagerTrait, WalletDerivationManagerTrait};
use crate::encryption::sha256_hash;
use crate::imports::*;
use crate::runtime::account::AccountKind;
use crate::storage::{self, PrvKeyDataId};
use kaspa_hashes::Hash;
use kaspa_utils::as_slice::AsSlice;
use secp256k1::PublicKey;

#[derive(BorshSerialize)]
struct AccountIdHashData<T: AsSlice<Element = PrvKeyDataId>> {
    account_kind: AccountKind,
    prv_key_data_id: Option<T>,
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
            prv_key_data_id: Some([*prv_key_data_id]),
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
            prv_key_data_id: Some([*prv_key_data_id]),
            ecdsa: Some(false),
            account_index: Some(0),
            secp256k1_public_key: None,
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub(crate) fn from_multisig(data: &storage::account::MultiSig) -> AccountId {
        let hashable = AccountIdHashData {
            account_kind: AccountKind::MultiSig,
            prv_key_data_id: data.prv_key_data_ids.as_ref().cloned(),
            ecdsa: Some(data.ecdsa),
            account_index: Some(0),
            secp256k1_public_key: None,
            data: Some(data.xpub_keys.iter().flat_map(|s| s.as_bytes()).cloned().collect()),
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub(crate) fn from_keypair(prv_key_data_id: &PrvKeyDataId, data: &storage::account::Keypair) -> AccountId {
        let hashable = AccountIdHashData {
            account_kind: AccountKind::Keypair,
            prv_key_data_id: Some([*prv_key_data_id]),
            ecdsa: Some(data.ecdsa),
            account_index: None,
            secp256k1_public_key: Some(data.public_key.serialize().to_vec()),
            data: None,
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

    pub fn from_public_key(account_kind: AccountKind, public_key: &PublicKey) -> Self {
        let hashable: AccountIdHashData<[PrvKeyDataId; 0]> = AccountIdHashData {
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
        let hashable: AccountIdHashData<[PrvKeyDataId; 0]> = AccountIdHashData {
            account_kind,
            prv_key_data_id: None,
            ecdsa: None,
            account_index: None,
            secp256k1_public_key: None,
            data: Some(data.to_vec()),
        };
        AccountId(Hash::from_slice(sha256_hash(&hashable.try_to_vec().unwrap()).as_ref()))
    }

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

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
