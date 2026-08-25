use thiserror::Error;

#[derive(Debug, Error)]
pub enum FieldsError {
    #[error("Invalid Fr length: expected 32 bytes, got {0}")]
    InvalidLength(usize),
    #[error("ARK serialization error: {0}")]
    ArkSerialization(#[from] ark_serialize::SerializationError),
}
