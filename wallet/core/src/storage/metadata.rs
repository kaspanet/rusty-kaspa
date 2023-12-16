//! AccountMetadata is an associative structure that contains
//! additional information about an account. This structure
//! is not encrypted and is stored in plain text. This is meant
//! to provide an ability to perform various operations (such as
//! new address generation) without the need to re-encrypt the
//! wallet data when storing.

use crate::derivation::AddressDerivationMeta;
use crate::imports::*;
use crate::storage::IdT;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountMetadata {
    pub id: AccountId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<AddressDerivationMeta>,
}

impl AccountMetadata {
    pub fn new(id: AccountId, indexes: AddressDerivationMeta) -> Self {
        Self { id, indexes: Some(indexes) }
    }

    pub fn address_derivation_indexes(&self) -> Option<AddressDerivationMeta> {
        self.indexes.clone()
    }
}

impl IdT for AccountMetadata {
    type Id = AccountId;
    fn id(&self) -> &AccountId {
        &self.id
    }
}
