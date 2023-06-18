use std::cmp::Eq;
use std::fmt::Debug;
use std::hash::Hash;
use kaspa_utils::hex::ToHex;

use crate::storage::{Account, AccountId, PrvKeyData, PrvKeyDataId, PrvKeyDataInfo, TransactionRecord, TransactionRecordId};

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
    type Id = TransactionRecordId;
    fn id(&self) -> &TransactionRecordId {
        &self.id
    }
}
