//!
//! Id references used to associate transactions with Account or UtxoContext ids.
//!

use crate::imports::*;
use crate::utxo::{UtxoContextBinding as UtxoProcessorBinding, UtxoContextId};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "id")]
pub enum Binding {
    Custom(UtxoContextId),
    Account(AccountId),
}

impl From<UtxoProcessorBinding> for Binding {
    fn from(b: UtxoProcessorBinding) -> Self {
        match b {
            UtxoProcessorBinding::Internal(id) => Binding::Custom(id),
            UtxoProcessorBinding::Id(id) => Binding::Custom(id),
            UtxoProcessorBinding::AccountId(id) => Binding::Account(id),
        }
    }
}

impl From<&Arc<dyn Account>> for Binding {
    fn from(account: &Arc<dyn Account>) -> Self {
        Binding::Account(*account.id())
    }
}

impl Binding {
    pub fn to_hex(&self) -> String {
        match self {
            Binding::Custom(id) => id.to_hex(),
            Binding::Account(id) => id.to_hex(),
        }
    }
}

impl AsRef<Binding> for Binding {
    fn as_ref(&self) -> &Binding {
        self
    }
}

impl TryFrom<JsValue> for Binding {
    type Error = Error;

    fn try_from(value: JsValue) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let binding_type = object.get_string("type")?;
            return match &*binding_type {
                "custom" => {
                    let id: UtxoContextId = object.get_value("id")?.try_into()?;
                    Ok(Binding::Custom(id))
                }
                "account" => {
                    let id: AccountId = object.get_value("id")?.try_into()?;
                    Ok(Binding::Account(id))
                }
                _ => Err(Error::Custom(format!("invalid binding type: {}", binding_type))),
            };
        } else {
            Err(Error::Custom("supplied argument must be an object".to_string()))
        }
    }
}
