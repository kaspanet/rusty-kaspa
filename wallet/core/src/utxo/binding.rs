use crate::imports::*;
use crate::utxo::UtxoContextId;
use runtime::AccountId;

#[derive(Clone)]
pub enum UtxoContextBinding {
    Internal(UtxoContextId),
    AccountId(AccountId),
    Id(UtxoContextId),
}

impl Default for UtxoContextBinding {
    fn default() -> Self {
        UtxoContextBinding::Internal(UtxoContextId::default())
    }
}

impl UtxoContextBinding {
    pub fn id(&self) -> UtxoContextId {
        match self {
            UtxoContextBinding::Internal(id) => *id,
            UtxoContextBinding::AccountId(id) => (*id).into(), //account.id().into(),
            UtxoContextBinding::Id(id) => *id,
        }
    }
}
