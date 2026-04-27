use thiserror::Error;

#[derive(Debug, Error)]
pub enum PointError {
    #[error("Malformed G1 field element")]
    MalformedG1,
    #[error("Malformed G2 field element")]
    MalformedG2,
    #[error("Ark deserialization error: {0}")]
    ArkDeserialization(#[from] ark_serialize::SerializationError),
}
