//!
//! Account descriptors (client-side account information representation).
//!

use crate::derivation::AddressDerivationMeta;
use crate::imports::*;
use borsh::{BorshDeserialize, BorshSerialize};
use convert_case::{Case, Casing};
use kaspa_addresses::Address;
use kaspa_wallet_macros::declare_typescript_wasm_interface as declare;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// @category Wallet API
#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountDescriptor {
    pub kind: AccountKind,
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
        Self { kind, account_id, account_name, prv_key_data_ids, receive_address, change_address, properties: BTreeMap::default() }
    }

    pub fn with_property(mut self, property: AccountDescriptorProperty, value: AccountDescriptorValue) -> Self {
        self.properties.insert(property, value);
        self
    }
}

#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccountDescriptorProperty {
    AccountIndex,
    XpubKeys,
    Ecdsa,
    DerivationMeta,
    Other(String),
}

impl std::fmt::Display for AccountDescriptorProperty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountDescriptorProperty::AccountIndex => write!(f, "Account Index"),
            AccountDescriptorProperty::XpubKeys => write!(f, "Xpub Keys"),
            AccountDescriptorProperty::Ecdsa => write!(f, "ECDSA"),
            AccountDescriptorProperty::DerivationMeta => write!(f, "Derivation Indexes"),
            AccountDescriptorProperty::Other(other) => write!(f, "{}", other),
        }
    }
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

impl TryFrom<AccountDescriptorValue> for JsValue {
    type Error = Error;
    fn try_from(value: AccountDescriptorValue) -> Result<Self> {
        let js_value = match value {
            AccountDescriptorValue::U64(value) => BigInt::from(value).into(),
            AccountDescriptorValue::String(value) => JsValue::from(value),
            AccountDescriptorValue::Bool(value) => JsValue::from(value),
            AccountDescriptorValue::AddressDerivationMeta(value) => {
                let object = Object::new();
                object.set("receive", &value.receive().into())?;
                object.set("change", &value.change().into())?;
                object.into()
            }
            AccountDescriptorValue::XPubKeys(value) => {
                let array = Array::new();
                for xpub in value.iter() {
                    array.push(&JsValue::from(xpub.to_string(None)));
                }
                array.into()
            }
            AccountDescriptorValue::Json(value) => JsValue::from(value),
        };

        Ok(js_value)
    }
}

impl std::fmt::Display for AccountDescriptorValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountDescriptorValue::U64(value) => write!(f, "{}", value),
            AccountDescriptorValue::String(value) => write!(f, "{}", value),
            AccountDescriptorValue::Bool(value) => write!(f, "{}", value),
            AccountDescriptorValue::AddressDerivationMeta(value) => write!(f, "{}", value),
            AccountDescriptorValue::XPubKeys(value) => {
                let mut s = String::new();
                for xpub in value.iter() {
                    s.push_str(&format!("{}\n", xpub));
                }
                write!(f, "{}", s)
            }
            AccountDescriptorValue::Json(value) => write!(f, "{}", value),
        }
    }
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

    pub fn account_kind(&self) -> &AccountKind {
        &self.kind
    }

    pub fn receive_address(&self) -> &Option<Address> {
        &self.receive_address
    }
}

declare! {
    IAccountDescriptor,
    r#"
    /**
     * 
     * 
     * @category Wallet API
     */
    export interface IAccountDescriptor {
        kind : AccountKind,
        accountId : HexString,
        accountName? : string,
        receiveAddress? : Address,
        changeAddress? : Address,
        prvKeyDataIds : HexString[],
        [key: string]: any
    }
    "#,
}

impl TryFrom<AccountDescriptor> for IAccountDescriptor {
    type Error = Error;
    fn try_from(descriptor: AccountDescriptor) -> Result<Self> {
        let object = IAccountDescriptor::default();

        object.set("kind", &descriptor.kind.into())?;
        object.set("accountId", &descriptor.account_id.into())?;
        object.set("accountName", &descriptor.account_name.into())?;
        object.set("receiveAddress", &descriptor.receive_address.into())?;
        object.set("changeAddress", &descriptor.change_address.into())?;

        let prv_key_data_ids = js_sys::Array::from_iter(descriptor.prv_key_data_ids.into_iter().map(JsValue::from));
        object.set("prvKeyDataIds", &prv_key_data_ids)?;

        // let properties = Object::new();
        for (property, value) in descriptor.properties {
            let ident = property.to_string().to_case(Case::Camel);
            object.set(&ident, &value.try_into()?)?;
        }

        Ok(object)
    }
}
