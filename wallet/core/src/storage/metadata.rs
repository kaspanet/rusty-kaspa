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
    const STORAGE_MAGIC: u32 = 0x4154454d;
    const STORAGE_VERSION: u32 = 0;

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

impl BorshSerialize for AccountMetadata {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(Self::STORAGE_MAGIC, Self::STORAGE_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.id, writer)?;
        BorshSerialize::serialize(&self.indexes, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for AccountMetadata {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(Self::STORAGE_MAGIC)?.try_version(Self::STORAGE_VERSION)?;

        let id = BorshDeserialize::deserialize(buf)?;
        let indexes = BorshDeserialize::deserialize(buf)?;

        Ok(Self { id, indexes })
    }
}
