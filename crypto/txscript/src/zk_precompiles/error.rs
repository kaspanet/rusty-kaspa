use kaspa_txscript_errors::TxScriptError;
use risc0_zkp::verify::VerificationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZkIntegrityError {
    #[error("ZK verification failed: {0}")]
    R0Verification(String),
    #[error("Std io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Txscript error: {0}")]
    TxScript(#[from] TxScriptError),
    #[error("Digest parsing error: {0:?}")]
    Digest(Vec<u8>),
    #[error("Mekle proof verification failed")]
    Merkle,
    #[error("Unknown tag: {0}")]
    UnknownTag(u8),
}

impl From<VerificationError> for ZkIntegrityError {
    fn from(err: VerificationError) -> Self {
        ZkIntegrityError::R0Verification(format!("{:?}", err))
    }
}
