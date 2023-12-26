//!
//! Implementation of [`UtxoContextBinding`] which allows binding of
//! [`UtxoContext`] to [`Account`] or custom developer-defined ids.
//!

use crate::imports::*;
use crate::utxo::UtxoContextId;

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
            UtxoContextBinding::AccountId(id) => (*id).into(),
            UtxoContextBinding::Id(id) => *id,
        }
    }
}
