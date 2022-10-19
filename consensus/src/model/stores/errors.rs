use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("key not found in store")]
    KeyNotFound(String),

    #[error("key already exists in store")]
    KeyAlreadyExists(String),

    #[error("rocksdb error")]
    DbError(#[from] rocksdb::Error),

    #[error("bincode error")]
    DeserializationError(#[from] Box<bincode::ErrorKind>),
}

pub type StoreResult<T> = std::result::Result<T, StoreError>;

pub trait StoreResultExtensions<T> {
    fn unwrap_option(self) -> Option<T>;
}

impl<T> StoreResultExtensions<T> for StoreResult<T> {
    fn unwrap_option(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(err) => panic!("Unexpected store error: {:?}", err),
        }
    }
}
