use crate::imports::*;
use crate::runtime::Account;
use crate::utxo::UtxoContextId;

#[derive(Clone)]
pub enum Binding {
    Internal(UtxoContextId),
    Account(Arc<Account>),
    Id(UtxoContextId),
}

impl Default for Binding {
    fn default() -> Self {
        Binding::Internal(UtxoContextId::default())
    }
}

impl Binding {
    pub fn id(&self) -> UtxoContextId {
        match self {
            Binding::Internal(id) => *id,
            Binding::Account(account) => account.id().into(),
            Binding::Id(id) => *id,
        }
    }
}
