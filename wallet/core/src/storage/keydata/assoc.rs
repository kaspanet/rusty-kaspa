use crate::imports::*;

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "data")]
pub enum AssocPrvKeyDataIds {
    None,
    Single(PrvKeyDataId),
    Multiple(Arc<Vec<PrvKeyDataId>>),
}

impl IntoIterator for &AssocPrvKeyDataIds {
    type Item = PrvKeyDataId;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            AssocPrvKeyDataIds::None => Vec::new().into_iter(),
            AssocPrvKeyDataIds::Single(id) => vec![*id].into_iter(),
            AssocPrvKeyDataIds::Multiple(ids) => (**ids).clone().into_iter(),
        }
    }
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

impl TryFrom<AssocPrvKeyDataIds> for Option<Arc<Vec<PrvKeyDataId>>> {
    type Error = Error;

    fn try_from(value: AssocPrvKeyDataIds) -> Result<Self> {
        match value {
            AssocPrvKeyDataIds::None => Ok(None),
            AssocPrvKeyDataIds::Multiple(ids) => Ok(Some(ids)),
            _ => Err(Error::AssocPrvKeyDataIds("None or Multiple".to_string(), value)),
        }
    }
}

impl AssocPrvKeyDataIds {
    pub fn contains(&self, id: &PrvKeyDataId) -> bool {
        match self {
            AssocPrvKeyDataIds::None => false,
            AssocPrvKeyDataIds::Single(single) => single == id,
            AssocPrvKeyDataIds::Multiple(multiple) => multiple.contains(id),
        }
    }
}
