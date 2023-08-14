use crate::imports::*;
use crate::storage::AccountId;
use crate::storage::IdT;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Meta {
    Derivation([u32; 2])
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    id : AccountId,
    meta : Meta,
}

impl Metadata {
    pub fn new(id : AccountId, meta : Meta) -> Self {
        Self {
            id,
            meta,
        }
    }
}

impl IdT for Metadata {
    type Id = AccountId;
    fn id(&self) -> &AccountId {
        &self.id
    }
}

