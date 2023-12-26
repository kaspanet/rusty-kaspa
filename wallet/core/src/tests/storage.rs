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
        let bytes = self.try_to_vec()?;
        let transform = Self::try_from_slice(bytes.as_slice())?;
        assert_eq!(transform.before, 0xdeadbeef);
        assert_eq!(transform.after, 0xbaadf00d);
        let transform_bytes = transform.try_to_vec()?;
        assert_eq!(bytes, transform_bytes);
        Ok(transform.storable)
    }
}
