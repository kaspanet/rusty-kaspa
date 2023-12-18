use crate::derivation::AddressDerivationMeta;
use crate::imports::account::AssocPrvKeyDataIds;
use crate::imports::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountDescriptor {
    pub kind: String,
    pub account_id: AccountId,
    pub account_name: Option<String>,
    pub prv_key_data_ids: AssocPrvKeyDataIds,
    pub receive_address: Option<Address>,
    pub change_address: Option<Address>,

    pub properties: BTreeMap<AccountDescriptorProperty, AccountDescriptorValue>,
}

impl AccountDescriptor {
    pub fn new(
        kind: AccountKind,
        account_id: AccountId,
        account_name: Option<String>,
        prv_key_data_ids: AssocPrvKeyDataIds,
        receive_address: Option<Address>,
        change_address: Option<Address>,
    ) -> Self {
        Self {
            kind: kind.to_string(),
            account_id,
            account_name,
            prv_key_data_ids,
            receive_address,
            change_address,
            properties: BTreeMap::default(),
        }
    }

    pub fn with_property(mut self, property: AccountDescriptorProperty, value: AccountDescriptorValue) -> Self {
        self.properties.insert(property, value);
        self
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]

pub enum AccountDescriptorProperty {
    AccountIndex,
    XpubKeys,
    Ecdsa,
    DerivationMeta,
    Other(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "value")]
#[serde(rename_all = "kebab-case")]
pub enum AccountDescriptorValue {
    U64(u64),
    String(String),
    Bool(bool),
    AddressDerivationMeta(AddressDerivationMeta),
    XPubKeys(ExtendedPublicKeys),
    Json(String),
}

impl From<u64> for AccountDescriptorValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl From<String> for AccountDescriptorValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<bool> for AccountDescriptorValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<AddressDerivationMeta> for AccountDescriptorValue {
    fn from(value: AddressDerivationMeta) -> Self {
        Self::AddressDerivationMeta(value)
    }
}

impl From<&str> for AccountDescriptorValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<ExtendedPublicKeys> for AccountDescriptorValue {
    fn from(value: ExtendedPublicKeys) -> Self {
        Self::XPubKeys(value)
    }
}

impl From<serde_json::Value> for AccountDescriptorValue {
    fn from(value: serde_json::Value) -> Self {
        Self::Json(value.to_string())
    }
}

impl AccountDescriptor {
    pub fn name(&self) -> &Option<String> {
        &self.account_name
    }

    pub fn prv_key_data_ids(&self) -> &AssocPrvKeyDataIds {
        &self.prv_key_data_ids
    }

    pub fn account_id(&self) -> &AccountId {
        &self.account_id
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

    pub fn account_kind(&self) -> &str {
        self.kind.as_str()
    }

    pub fn receive_address(&self) -> &Option<Address> {
        &self.receive_address
    }
}
