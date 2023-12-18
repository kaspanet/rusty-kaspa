//!
//! Deterministic byte sequence generation (used by Account ids).
//! 

pub use crate::account::{bip32, keypair, legacy, multisig};
use crate::encryption::sha256_hash;
use crate::imports::*;
use crate::storage::PrvKeyDataId;
use kaspa_hashes::Hash;
use kaspa_utils::as_slice::AsSlice;
use secp256k1::PublicKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountStorageKey(pub(crate) Hash);

impl AccountStorageKey {
    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("[{}]", &hex[0..8])
    }
}

impl ToHex for AccountStorageKey {
    fn to_hex(&self) -> String {
        format!("{}", self.0)
    }
}

impl std::fmt::Display for AccountStorageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountId(pub(crate) Hash);

impl AccountId {
    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("[{}]", &hex[0..8])
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

#[derive(BorshSerialize)]
struct DeterministicHashData<'data, T: AsSlice<Element = PrvKeyDataId>> {
    account_kind: &'data AccountKind,
    prv_key_data_ids: &'data Option<T>,
    ecdsa: Option<bool>,
    account_index: Option<u64>,
    secp256k1_public_key: Option<Vec<u8>>,
    data: Option<Vec<u8>>,
}

#[inline(always)]
pub(crate) fn make_account_hashes(v: [Hash; 2]) -> (AccountId, AccountStorageKey) {
    (AccountId(v[1]), AccountStorageKey(v[0]))
}

fn make_hashes<T, const N: usize>(hashable: DeterministicHashData<T>) -> [Hash; N]
where
    T: AsSlice<Element = PrvKeyDataId> + BorshSerialize,
{
    let mut hashes: [Hash; N] = [Hash::default(); N];
    let bytes = hashable.try_to_vec().unwrap();
    hashes[0] = Hash::from_slice(sha256_hash(&bytes).as_ref());
    for i in 1..N {
        hashes[i] = Hash::from_slice(sha256_hash(&hashes[i - 1].as_bytes()).as_ref());
    }
    hashes
}

pub fn from_bip32<const N: usize>(prv_key_data_id: &PrvKeyDataId, data: &bip32::Storable) -> [Hash; N] {
    let hashable = DeterministicHashData {
        account_kind: &bip32::BIP32_ACCOUNT_KIND.into(),
        prv_key_data_ids: &Some([*prv_key_data_id]),
        ecdsa: Some(data.ecdsa),
        account_index: Some(data.account_index),
        secp256k1_public_key: None,
        data: None,
    };
    make_hashes(hashable)
}

pub fn from_legacy<const N: usize>(prv_key_data_id: &PrvKeyDataId, _data: &legacy::Storable) -> [Hash; N] {
    let hashable = DeterministicHashData {
        account_kind: &legacy::LEGACY_ACCOUNT_KIND.into(),
        prv_key_data_ids: &Some([*prv_key_data_id]),
        ecdsa: Some(false),
        account_index: Some(0),
        secp256k1_public_key: None,
        data: None,
    };
    make_hashes(hashable)
}

pub fn from_multisig<const N: usize>(prv_key_data_ids: &Option<Arc<Vec<PrvKeyDataId>>>, data: &multisig::Storable) -> [Hash; N] {
    let hashable = DeterministicHashData {
        account_kind: &multisig::MULTISIG_ACCOUNT_KIND.into(),
        prv_key_data_ids,
        ecdsa: Some(data.ecdsa),
        account_index: None,
        secp256k1_public_key: None,
        data: Some(data.xpub_keys.try_to_vec().unwrap()),
    };
    make_hashes(hashable)
}

pub(crate) fn from_keypair<const N: usize>(prv_key_data_id: &PrvKeyDataId, data: &keypair::Storable) -> [Hash; N] {
    let hashable = DeterministicHashData {
        account_kind: &keypair::KEYPAIR_ACCOUNT_KIND.into(),
        prv_key_data_ids: &Some([*prv_key_data_id]),
        ecdsa: Some(data.ecdsa),
        account_index: None,
        secp256k1_public_key: Some(data.public_key.serialize().to_vec()),
        data: None,
    };
    make_hashes(hashable)
}

pub fn from_public_key<const N: usize>(account_kind: &AccountKind, public_key: &PublicKey) -> [Hash; N] {
    let hashable: DeterministicHashData<[PrvKeyDataId; 0]> = DeterministicHashData {
        account_kind,
        prv_key_data_ids: &None,
        ecdsa: None,
        account_index: None,
        secp256k1_public_key: Some(public_key.serialize().to_vec()),
        data: None,
    };
    make_hashes(hashable)
}

pub fn from_data<const N: usize>(account_kind: &AccountKind, data: &[u8]) -> [Hash; N] {
    let hashable: DeterministicHashData<[PrvKeyDataId; 0]> = DeterministicHashData {
        account_kind,
        prv_key_data_ids: &None,
        ecdsa: None,
        account_index: None,
        secp256k1_public_key: None,
        data: Some(data.to_vec()),
    };
    make_hashes(hashable)
}
