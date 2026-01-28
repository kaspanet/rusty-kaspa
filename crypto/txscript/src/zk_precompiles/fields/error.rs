use thiserror::Error;

#[derive(Debug, Error)]
pub enum FieldsError {
    #[error("ARK serialization error: {0}")]
    ArkSerialization(#[from] ark_serialize::SerializationError),
}
