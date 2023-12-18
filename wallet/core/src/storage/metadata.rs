//! AccountMetadata is an associative structure that contains
//! additional information about an account. This structure
//! is not encrypted and is stored in plain text. This is meant
//! to provide an ability to perform various operations (such as
//! new address generation) without the need to re-encrypt the
//! wallet data when storing.

use crate::derivation::AddressDerivationMeta;
use crate::imports::*;
use crate::storage::IdT;

const ACCOUNT_METADATA_MAGIC: u32 = 0x4d455441;
const ACCOUNT_METADATA_VERSION: u32 = 0;

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

impl BorshSerialize for AccountMetadata {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(ACCOUNT_METADATA_MAGIC, ACCOUNT_METADATA_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.id, writer)?;
        BorshSerialize::serialize(&self.indexes, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for AccountMetadata {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(ACCOUNT_METADATA_MAGIC)?.try_version(ACCOUNT_METADATA_VERSION)?;

        let id = BorshDeserialize::deserialize(buf)?;
        let indexes = BorshDeserialize::deserialize(buf)?;

        Ok(Self { id, indexes })
    }
}
