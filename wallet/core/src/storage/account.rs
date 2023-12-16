use crate::imports::*;
use crate::result::Result;
use crate::storage::PrvKeyDataId;

#[derive(Default, Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
pub struct AccountSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "data")]
pub enum AssocPrvKeyDataIds {
    None,
    Single(PrvKeyDataId),
    Multiple(Arc<Vec<PrvKeyDataId>>),
}

impl From<PrvKeyDataId> for AssocPrvKeyDataIds {
    fn from(value: PrvKeyDataId) -> Self {
        AssocPrvKeyDataIds::Single(value)
    }
}

impl TryFrom<Option<Arc<Vec<PrvKeyDataId>>>> for AssocPrvKeyDataIds {
    type Error = Error;

    fn try_from(value: Option<Arc<Vec<PrvKeyDataId>>>) -> Result<Self> {
        match value {
            None => Ok(AssocPrvKeyDataIds::None),
            Some(ids) => {
                if ids.is_empty() {
                    return Err(Error::AssocPrvKeyDataIdsEmpty);
                }
                Ok(AssocPrvKeyDataIds::Multiple(ids))
            }
        }
    }
}

impl TryFrom<AssocPrvKeyDataIds> for PrvKeyDataId {
    type Error = Error;

    fn try_from(value: AssocPrvKeyDataIds) -> Result<Self> {
        match value {
            AssocPrvKeyDataIds::Single(id) => Ok(id),
            _ => Err(Error::AssocPrvKeyDataIds("Single".to_string(), value)),
        }
    }
}

impl TryFrom<AssocPrvKeyDataIds> for Arc<Vec<PrvKeyDataId>> {
    type Error = Error;

    fn try_from(value: AssocPrvKeyDataIds) -> Result<Self> {
        match value {
            AssocPrvKeyDataIds::Multiple(ids) => Ok(ids),
            _ => Err(Error::AssocPrvKeyDataIds("Multiple".to_string(), value)),
        }
    }
}

impl AssocPrvKeyDataIds {
    pub fn contains(&self, id: &PrvKeyDataId) -> bool {
        match self {
            AssocPrvKeyDataIds::None => false,
            AssocPrvKeyDataIds::Single(single) => single == id,
            AssocPrvKeyDataIds::Multiple(multiple) => multiple.iter().any(|elem| elem == id),
        }
    }
}

const ACCOUNT_VERSION: u32 = 0;
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct AccountStorage {
    #[serde(default)]
    pub version: [u32; 2],

    pub kind: String,
    pub id: AccountId,
    pub storage_key: AccountStorageKey,
    pub prv_key_data_ids: AssocPrvKeyDataIds,
    pub settings: AccountSettings,
    pub serialized: Vec<u8>,
}

impl AccountStorage {
    pub fn new(
        kind: &str,
        data_version: u32,
        id: &AccountId,
        storage_key: &AccountStorageKey,
        prv_key_data_ids: AssocPrvKeyDataIds,
        settings: AccountSettings,
        serialized: &[u8],
    ) -> Self {
        Self {
            version: [ACCOUNT_VERSION, data_version],
            id: *id,
            storage_key: *storage_key,
            kind: kind.to_string(),
            prv_key_data_ids,
            settings,
            serialized: serialized.to_vec(),
        }
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
