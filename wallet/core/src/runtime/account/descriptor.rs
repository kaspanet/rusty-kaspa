use crate::runtime::account::{AccountId, AccountKind};
use crate::{derivation::AddressDerivationMeta, storage::PrvKeyDataId};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Bip32 {
    pub account_id: AccountId,
    pub account_name: Option<String>,
    pub prv_key_data_id: PrvKeyDataId,
    pub account_index: u64,
    pub xpub_keys: Arc<Vec<String>>,
    pub ecdsa: bool,
    pub receive_address: Option<Address>,
    pub change_address: Option<Address>,
    pub meta: AddressDerivationMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Keypair {
    pub account_id: AccountId,
    pub account_name: Option<String>,
    pub prv_key_data_id: PrvKeyDataId,
    pub public_key: String,
    pub ecdsa: bool,
    pub receive_address: Option<Address>,
    pub change_address: Option<Address>,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Legacy {
    pub account_id: AccountId,
    pub account_name: Option<String>,
    pub prv_key_data_id: PrvKeyDataId,
    // pub xpub_keys: Arc<Vec<String>>,
    pub receive_address: Option<Address>,
    pub change_address: Option<Address>,
    pub meta: AddressDerivationMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct MultiSig {
    pub account_id: AccountId,
    pub account_name: Option<String>,
    // TODO add multisig data
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Resident {
    pub account_id: AccountId,
    pub account_name: Option<String>,
    pub public_key: String,
}

/// [`Descriptor`] is a type that offers serializable representation of an account.
/// It is means for RPC or transports that do not allow for transport
/// of the account data.
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
pub enum AccountDescriptor {
    Bip32(Bip32),
    Keypair(Keypair),
    Legacy(Legacy),
    MultiSig(MultiSig),
    Resident(Resident),
}

impl From<Bip32> for AccountDescriptor {
    fn from(bip32: Bip32) -> Self {
        Self::Bip32(bip32)
    }
}

impl From<Keypair> for AccountDescriptor {
    fn from(keypair: Keypair) -> Self {
        Self::Keypair(keypair)
    }
}

impl From<Legacy> for AccountDescriptor {
    fn from(legacy: Legacy) -> Self {
        Self::Legacy(legacy)
    }
}

impl From<MultiSig> for AccountDescriptor {
    fn from(multisig: MultiSig) -> Self {
        Self::MultiSig(multisig)
    }
}

impl From<Resident> for AccountDescriptor {
    fn from(resident: Resident) -> Self {
        Self::Resident(resident)
    }
}

impl AccountDescriptor {
    pub fn name(&self) -> &Option<String> {
        match self {
            AccountDescriptor::Bip32(bip32) => &bip32.account_name,
            AccountDescriptor::Keypair(keypair) => &keypair.account_name,
            AccountDescriptor::Legacy(legacy) => &legacy.account_name,
            AccountDescriptor::MultiSig(multisig) => &multisig.account_name,
            AccountDescriptor::Resident(resident) => &resident.account_name,
        }
    }

    // pub fn prv_key_data_id(&self) -> &PrvKeyDataId {
    //     match self {
    //         AccountDescriptor::Bip32(bip32) => &bip32.prv_key_data_id,
    //         AccountDescriptor::Keypair(keypair) => &keypair.prv_key_data_id,
    //         AccountDescriptor::Legacy(legacy) => &legacy.prv_key_data_id,
    //         AccountDescriptor::MultiSig(multisig) => &multisig.prv_key_data_id,
    //         AccountDescriptor::Resident(resident) => &resident.prv_key_data_id,
    //     }
    // }

    pub fn account_id(&self) -> &AccountId {
        match self {
            AccountDescriptor::Bip32(bip32) => &bip32.account_id,
            AccountDescriptor::Keypair(keypair) => &keypair.account_id,
            AccountDescriptor::Legacy(legacy) => &legacy.account_id,
            AccountDescriptor::MultiSig(multisig) => &multisig.account_id,
            AccountDescriptor::Resident(resident) => &resident.account_id,
        }
    }

    pub fn name_or_id(&self) -> String {
        if let Some(name) = self.name() {
            if name.is_empty() {
                self.account_id().short()
            } else {
                name.clone()
            }
        } else {
            self.account_id().short()
        }
    }

    pub fn name_with_id(&self) -> String {
        if let Some(name) = self.name() {
            if name.is_empty() {
                self.account_id().short()
            } else {
                format!("{name} {}", self.account_id().short())
            }
        } else {
            self.account_id().short()
        }
    }

    pub fn account_kind(&self) -> AccountKind {
        match self {
            AccountDescriptor::Bip32(_) => AccountKind::Bip32,
            AccountDescriptor::Keypair(_) => AccountKind::Keypair,
            AccountDescriptor::Legacy(_) => AccountKind::Legacy,
            AccountDescriptor::MultiSig(_) => AccountKind::MultiSig,
            AccountDescriptor::Resident(_) => AccountKind::Resident,
        }
    }

    pub fn receive_address(&self) -> Option<Address> {
        match self {
            AccountDescriptor::Bip32(bip32) => bip32.receive_address.clone(),
            AccountDescriptor::Keypair(keypair) => keypair.receive_address.clone(),
            AccountDescriptor::Legacy(legacy) => legacy.receive_address.clone(),
            // Descriptor::MultiSig(_) => Err(Error::UnsupportedAccountKind),
            // Descriptor::Resident(_) => Err(Error::UnsupportedAccountKind),
            _ => {
                todo!("TODO multisig and resident")
            }
        }
    }
}
