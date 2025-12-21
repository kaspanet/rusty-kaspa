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
    #[error("Verification failed")]
    VerificationFailed,
    #[error("Merkle proof verification failed")]
    Merkle,
}
