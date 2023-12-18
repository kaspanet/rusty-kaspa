//!
//! Storage wrapper for account data.
//!

use crate::imports::*;

const ACCOUNT_SETTINGS_VERSION: u32 = 0;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct AccountSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Vec<u8>>,
}

impl BorshSerialize for AccountSettings {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        BorshSerialize::serialize(&ACCOUNT_SETTINGS_VERSION, writer)?;
        BorshSerialize::serialize(&self.name, writer)?;
        BorshSerialize::serialize(&self.meta, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for AccountSettings {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let _version: u32 = BorshDeserialize::deserialize(buf)?;
        let name = BorshDeserialize::deserialize(buf)?;
        let meta = BorshDeserialize::deserialize(buf)?;

        Ok(Self { name, meta })
    }
}

const ACCOUNT_MAGIC: u32 = 0x4B415341;
const ACCOUNT_VERSION: u32 = 0;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStorage {
    pub kind: AccountKind,
    pub id: AccountId,
    pub storage_key: AccountStorageKey,
    pub prv_key_data_ids: AssocPrvKeyDataIds,
    pub settings: AccountSettings,
    pub serialized: Vec<u8>,
}

impl AccountStorage {
    pub fn new(
        kind: AccountKind,
        id: &AccountId,
        storage_key: &AccountStorageKey,
        prv_key_data_ids: AssocPrvKeyDataIds,
        settings: AccountSettings,
        serialized: &[u8],
    ) -> Self {
        Self { id: *id, storage_key: *storage_key, kind, prv_key_data_ids, settings, serialized: serialized.to_vec() }
    }

    pub fn id(&self) -> &AccountId {
        &self.id
    }

    pub fn storage_key(&self) -> &AccountStorageKey {
        &self.storage_key
    }

    pub fn serialized(&self) -> &[u8] {
        &self.serialized
    }
}

impl BorshSerialize for AccountStorage {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        StorageHeader::new(ACCOUNT_MAGIC, ACCOUNT_VERSION).serialize(writer)?;
        BorshSerialize::serialize(&self.kind, writer)?;
        BorshSerialize::serialize(&self.id, writer)?;
        BorshSerialize::serialize(&self.storage_key, writer)?;
        BorshSerialize::serialize(&self.prv_key_data_ids, writer)?;
        BorshSerialize::serialize(&self.settings, writer)?;
        BorshSerialize::serialize(&self.serialized, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for AccountStorage {
    fn deserialize(buf: &mut &[u8]) -> IoResult<Self> {
        let StorageHeader { version: _, .. } =
            StorageHeader::deserialize(buf)?.try_magic(ACCOUNT_MAGIC)?.try_version(ACCOUNT_VERSION)?;

        let kind = BorshDeserialize::deserialize(buf)?;
        let id = BorshDeserialize::deserialize(buf)?;
        let storage_key = BorshDeserialize::deserialize(buf)?;
        let prv_key_data_ids = BorshDeserialize::deserialize(buf)?;
        let settings = BorshDeserialize::deserialize(buf)?;
        let serialized = BorshDeserialize::deserialize(buf)?;

        Ok(Self { kind, id, storage_key, prv_key_data_ids, settings, serialized })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[test]
    fn test_storage_account_storage_wrapper() -> Result<()> {
        let (id, storage_key) = make_account_hashes(from_data(&BIP32_ACCOUNT_KIND.into(), &[0x00, 0x01, 0x02, 0x03]));
        let prv_key_data_id = PrvKeyDataId::new(0xcafe);
        let storable_in = AccountStorage::new(
            BIP32_ACCOUNT_KIND.into(),
            &id,
            &storage_key,
            prv_key_data_id.into(),
            AccountSettings::default(),
            &[0x00, 0x01, 0x02, 0x03],
        );
        let guard = StorageGuard::new(&storable_in);
        let storable_out = guard.validate()?;

        assert_eq!(storable_in.kind, storable_out.kind);
        assert_eq!(storable_in.id, storable_out.id);
        assert_eq!(storable_in.storage_key, storable_out.storage_key);
        assert_eq!(storable_in.serialized, storable_out.serialized);

        Ok(())
    }
}
