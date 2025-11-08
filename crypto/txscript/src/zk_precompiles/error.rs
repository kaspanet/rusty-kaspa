use risc0_zkp::verify::VerificationError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZkIntegrityError {
    #[error("ZK verification failed: {0}")]
    R0Verification(String),
    #[error("Unknown ZkPrecompile tag {0}")]
    UnknownZkPrecompileTag(String),
    #[error("Std io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<VerificationError> for ZkIntegrityError {
    fn from(err: VerificationError) -> Self {
        ZkIntegrityError::R0Verification(format!("{:?}", err))
    }
}
