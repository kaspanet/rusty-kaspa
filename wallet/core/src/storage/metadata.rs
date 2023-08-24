use crate::derivation::AddressDerivationMeta;
use crate::imports::*;
use crate::storage::AccountId;
use crate::storage::IdT;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub id: AccountId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<AddressDerivationMeta>,
}

impl Metadata {
    pub fn new(id: AccountId, indexes: AddressDerivationMeta) -> Self {
        Self { id, indexes: Some(indexes) }
    }

    pub fn address_derivation_indexes(&self) -> Option<AddressDerivationMeta> {
        self.indexes.clone()
    }
}

impl IdT for Metadata {
    type Id = AccountId;
    fn id(&self) -> &AccountId {
        &self.id
    }
}
