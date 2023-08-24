use crate::imports::*;
use crate::runtime::{Account, AccountId};
use crate::utxo::{UtxoContextBinding as UtxoProcessorBinding, UtxoContextId};

#[derive(Clone, Debug, Serialize, Deserialize)]
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
