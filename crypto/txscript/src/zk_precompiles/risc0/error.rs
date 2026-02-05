use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::verify::VerificationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum R0Error {
    #[error("Std io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("R0: {0}")]
    R0(#[from] VerificationError),
    #[error("Txscript error: {0}")]
    TxScript(#[from] TxScriptError),
    #[error("Digest parsing error: {0:?}")]
    Digest(Vec<u8>),
    #[error("Invalid seal length: {0}")]
    InvalidSealLength(usize),
    #[error("Invalid digest list length: {0}")]
    InvalidDigestLength(usize),
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
}
