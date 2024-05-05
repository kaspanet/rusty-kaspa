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

/// Deterministic byte sequence derived from account data (can be used for auxiliary data storage encryption).
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

/// Deterministic Account Id derived from account data.
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

impl FromHex for AccountId {
    type Error = Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        Ok(Self(Hash::from_hex(hex_str)?))
    }
}

impl TryFrom<&JsValue> for AccountId {
    type Error = Error;
    fn try_from(value: &JsValue) -> Result<Self> {
        let string = value.as_string().ok_or(Error::InvalidAccountId(format!("{value:?}")))?;
        Self::from_hex(&string)
    }
}

impl From<AccountId> for JsValue {
    fn from(value: AccountId) -> Self {
        JsValue::from(value.to_hex())
    }
}

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

seal! { 0x544d, {
    // IMPORTANT: This data structure is meant to be deterministic
    // so it can not contain any new fields or be changed.
    #[derive(BorshSerialize)]
    struct DeterministicHashData<'data, T: AsSlice<Element = PrvKeyDataId>> {
        account_kind: &'data AccountKind,
        prv_key_data_ids: &'data Option<T>,
        ecdsa: Option<bool>,
        account_index: Option<u64>,
        secp256k1_public_key: Option<Vec<u8>>,
        data: Option<Vec<u8>>,
    }
}}

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

/// Create deterministic hashes from BIP32 account data.
pub fn from_bip32<const N: usize>(prv_key_data_id: &PrvKeyDataId, data: &bip32::Payload) -> [Hash; N] {
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

/// Create deterministic hashes from legacy account data.
pub fn from_legacy<const N: usize>(prv_key_data_id: &PrvKeyDataId, _data: &legacy::Payload) -> [Hash; N] {
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

/// Create deterministic hashes from multisig account data.
pub fn from_multisig<const N: usize>(prv_key_data_ids: &Option<Arc<Vec<PrvKeyDataId>>>, data: &multisig::Payload) -> [Hash; N] {
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

/// Create deterministic hashes from keypair account data.
pub(crate) fn from_keypair<const N: usize>(prv_key_data_id: &PrvKeyDataId, data: &keypair::Payload) -> [Hash; N] {
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

/// Create deterministic hashes from a public key.
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

/// Create deterministic hashes from arbitrary data (supplied data slice must be deterministic).
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
