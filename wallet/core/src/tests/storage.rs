use crate::result::Result;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshSerialize, BorshDeserialize)]
pub struct StorageGuard<T>
where
    T: Clone + BorshSerialize + BorshDeserialize,
{
    pub before: u32,
    pub storable: T,
    pub after: u32,
}

impl<T> StorageGuard<T>
where
    T: Clone + BorshSerialize + BorshDeserialize,
{
    pub fn new(storable: &T) -> Self {
        Self { before: 0xdeadbeef, storable: storable.clone(), after: 0xbaadf00d }
    }

    pub fn validate(&self) -> Result<T> {
        let bytes = borsh::to_vec(self)?;
        let transform = Self::try_from_slice(bytes.as_slice())?;
        assert_eq!(transform.before, 0xdeadbeef);
        assert_eq!(transform.after, 0xbaadf00d);
        let transform_bytes = borsh::to_vec(&transform)?;
        assert_eq!(bytes, transform_bytes);
        Ok(transform.storable)
    }
}
