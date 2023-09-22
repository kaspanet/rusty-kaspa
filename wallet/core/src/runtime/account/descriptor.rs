use crate::runtime::account::AccountId;
use crate::{derivation::AddressDerivationMeta, storage::PrvKeyDataId};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Bip32 {
    pub account_id: AccountId,
    pub prv_key_data_id: PrvKeyDataId,
    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub ecdsa: bool,
    pub receive_address: Address,
    pub change_address: Address,
    pub meta: AddressDerivationMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Keypair {
    pub account_id: AccountId,
    pub prv_key_data_id: PrvKeyDataId,
    pub public_key: String,
    pub ecdsa: bool,
    pub address: Address,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Legacy {
    pub account_id: AccountId,
    pub prv_key_data_id: PrvKeyDataId,
    pub xpub_keys: Arc<Vec<String>>,
    pub receive_address: Address,
    pub change_address: Address,
    pub meta: AddressDerivationMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MultiSig {
    pub account_id: AccountId,
    // TODO add multisig data
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Resident {
    pub account_id: AccountId,
    pub public_key: String,
}

/// [`Descriptor`] is a type that offers serializable representation of an account.
/// It is means for RPC or transports that do not allow for transport
/// of the account data.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
pub enum Descriptor {
    Bip32(Bip32),
    Keypair(Keypair),
    Legacy(Legacy),
    MultiSig(MultiSig),
    Resident(Resident),
}

impl From<Bip32> for Descriptor {
    fn from(bip32: Bip32) -> Self {
        Self::Bip32(bip32)
    }
}

impl From<Keypair> for Descriptor {
    fn from(keypair: Keypair) -> Self {
        Self::Keypair(keypair)
    }
}

impl From<Legacy> for Descriptor {
    fn from(legacy: Legacy) -> Self {
        Self::Legacy(legacy)
    }
}

impl From<MultiSig> for Descriptor {
    fn from(multisig: MultiSig) -> Self {
        Self::MultiSig(multisig)
    }
}

impl From<Resident> for Descriptor {
    fn from(resident: Resident) -> Self {
        Self::Resident(resident)
    }
}
