use kaspa_consensus_core::tx::TransactionId;
use kaspa_utils::hex::ToHex;
use std::cmp::Eq;
use std::fmt::Debug;
use std::hash::Hash;

use crate::storage::{Account, AccountId, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, TransactionRecord};

pub trait IdT {
    type Id: Eq + Hash + Debug + ToHex;
    fn id(&self) -> &Self::Id;
}

impl IdT for PrvKeyData {
    type Id = PrvKeyDataId;
    fn id(&self) -> &PrvKeyDataId {
        &self.id
    }
}

impl IdT for PrvKeyDataInfo {
    type Id = PrvKeyDataId;
    fn id(&self) -> &PrvKeyDataId {
        &self.id
    }
}

impl IdT for Account {
    type Id = AccountId;
    fn id(&self) -> &AccountId {
        &self.id
    }
}

// impl IdT for Metadata {
//     type Id = AccountId;
//     fn id(&self) -> &AccountId {
//         &self.id
//     }
// }

impl IdT for TransactionRecord {
    type Id = TransactionId;
    fn id(&self) -> &TransactionId {
        &self.id
    }
}
