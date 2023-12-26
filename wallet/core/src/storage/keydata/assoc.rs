use crate::imports::*;
use itertools::Either;
use std::iter::{empty, once, Empty, Once};

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
    type IntoIter = Either<Either<Empty<Self::Item>, Once<Self::Item>>, std::vec::IntoIter<Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            AssocPrvKeyDataIds::None => Either::Left(Either::Left(empty())),
            AssocPrvKeyDataIds::Single(id) => Either::Left(Either::Right(once(*id))),
            AssocPrvKeyDataIds::Multiple(ids) => Either::Right((**ids).clone().into_iter()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assoc_prv_key_data_ids() -> Result<()> {
        let id = PrvKeyDataId::new(0x1ee7c0de);
        let vec = vec![PrvKeyDataId::new(0x1ee7c0de), PrvKeyDataId::new(0xbaadc0de), PrvKeyDataId::new(0xba5ec0de)];

        let iter = AssocPrvKeyDataIds::Single(id).into_iter();
        iter.for_each(|id| assert_eq!(id, id));

        let iter = AssocPrvKeyDataIds::Multiple(vec.clone().into()).into_iter();
        for (idx, id) in iter.enumerate() {
            assert_eq!(id, vec[idx]);
        }

        Ok(())
    }
}
