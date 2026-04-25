use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::verify::VerificationError;
use thiserror::Error;

use crate::zk_precompiles::fields::error::FieldsError;

#[derive(Debug, Error)]
pub enum R0Error {
    #[error("Std io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("R0: {0}")]
    R0(#[from] VerificationError),
    #[error("Txscript error: {0}")]
    TxScript(#[from] TxScriptError),
    #[error("Invalid digest length: {0}")]
    InvalidDigestLength(usize),
    #[error("Invalid seal length: {0}")]
    InvalidSealLength(usize),
    #[error("Invalid digest list length: {0}")]
    InvalidDigestListLength(usize),
    #[error("Invalid merkle index length: {0}")]
    InvalidMerkleIndexLength(usize),
    #[error("Invalid hash function encoding length: {0}")]
    InvalidHashFnEncoding(usize),
    #[error("Invalid hash function id: {0}")]
    InvalidHashFnId(u8),
    #[error("Verification failed")]
    VerificationFailed,
    #[error("Merkle proof verification failed")]
    Merkle,
    #[error("Invalid BabyBearElem in seal")]
    SealHasInvalidBabyBearElem,
    #[error("Script builder error: {0}")]
    ScriptBuilder(#[from] crate::script_builder::ScriptBuilderError),

    #[error("Fields error: {0}")]
    Fields(#[from] FieldsError),

    #[error("Seal decoding error: {0}")]
    SealDecoding(String),

    #[error("Bincode VK serialization failed")]
    BincodeVkSerialization,

    #[error("Point error: {0}")]
    Point(#[from] crate::zk_precompiles::points::PointError),

    #[error("Ark serialization error: {0}")]
    ArkSerialization(#[from] ark_serialize::SerializationError),
    #[error("Parse bigint error: {0}")]
    ParseBigInt(#[from] num_bigint::ParseBigIntError),
}
