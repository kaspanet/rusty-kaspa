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
