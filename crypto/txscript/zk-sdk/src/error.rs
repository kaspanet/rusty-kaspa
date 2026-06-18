use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Point error: {0}")]
    Point(#[from] crate::points::PointError),

    #[error("Seal decoding error: {0}")]
    SealDecoding(String),

    #[error("Ark serialization error: {0}")]
    ArkSerialization(#[from] ark_serialize::SerializationError),

    #[error("Script builder error: {0}")]
    ScriptBuilder(#[from] kaspa_txscript::script_builder::ScriptBuilderError),

    #[error("R0 verifying key serialization error")]
    VkSerialization,
}
