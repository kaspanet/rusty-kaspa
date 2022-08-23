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
    // More usage examples:
    //
    // #[error("data store disconnected")]
    // Disconnect(#[from] io::Error),
    // #[error("the data for key `{0}` is not available")]
    // Redaction(String),
    // #[error("invalid header (expected {expected:?}, found {found:?})")]
    // InvalidHeader {
    //     expected: String,
    //     found: String,
    // },
    // #[error("unknown data store error")]
    // Unknown,
}

pub type StoreResult<T> = std::result::Result<T, StoreError>;

pub trait StoreResultExtensions<T> {
    fn unwrap_option(self) -> Option<T>;
}

impl<T> StoreResultExtensions<T> for StoreResult<T> {
    fn unwrap_option(self) -> Option<T> {
        if let Err(StoreError::KeyNotFound(_)) = self {
            return None;
        }

        Some(self.unwrap())
    }
}
