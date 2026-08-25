use thiserror::Error;

#[derive(Debug, Error)]
pub enum Groth16Error {
    #[error("ARK R1CS error: {0}")]
    ArkR1CS(#[from] ark_relations::gr1cs::SynthesisError),
    #[error("Groth16 verification failed")]
    VerificationFailed,
    #[error("Groth16 verifying key has empty gamma_abc_g1")]
    EmptyGammaAbc,
    #[error("Groth16 verifying key bytes are too short to read gamma_abc_g1 length")]
    MalformedVerifyingKey,
    #[error("Groth16 verifying key has trailing bytes")]
    TrailingVerifyingKeyBytes,
    #[error("Groth16 proof has trailing bytes")]
    TrailingProofBytes,
    #[error("Kaspa txscript error: {0}")]
    FromTxScript(#[from] kaspa_txscript_errors::TxScriptError),
    #[error("ARK serialization error: {0}")]
    ArkSerialization(#[from] ark_serialize::SerializationError),
    #[error("Byte conversion error: {0}")]
    ByteConversion(#[from] std::array::TryFromSliceError),
    #[error("Field error: {0}")]
    FieldError(#[from] crate::zk_precompiles::fields::error::FieldsError),
}
