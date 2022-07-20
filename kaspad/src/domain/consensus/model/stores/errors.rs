use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Key not found in store")]
    KeyNotFound,
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
