use ark_serialize::SerializationError;
use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::verify::VerificationError;
use thiserror::Error;

use crate::{
    script_builder::ScriptBuilderError,
    zk_precompiles::{points::PointError, risc0::rcpt::HashFnId},
};

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
    #[error("Control inclusion proof length {actual} exceeds maximum {max}")]
    ControlInclusionProofTooLong { actual: usize, max: usize },
    #[error("Invalid merkle index length: {0}")]
    InvalidMerkleIndexLength(usize),
    #[error("Invalid hash function encoding length: {0}")]
    InvalidHashFnEncoding(usize),
    #[error("Invalid hash function id: {0}")]
    InvalidHashFnId(u8),
    #[error("Unsupported hash function: {0:?}")]
    UnsupportedHashFn(HashFnId),
    #[error("Verification failed")]
    VerificationFailed,
    #[error("Merkle proof verification failed")]
    Merkle,

    #[error("Point error: {0}")]
    PointError(#[from] PointError),

    #[error("Seal decoding error: {0}")]
    SealDecoding(String),

    #[error("Ark serialization error: {0}")]
    ArkSerialization(#[from] SerializationError),

    #[error("Script builder error: {0}")]
    ScriptBuilder(#[from] ScriptBuilderError),

    #[error("Bincode VK serialization error")]
    BincodeVkSerialization,
}
