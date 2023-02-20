use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum PruningError {
    #[error("pruning proof validation failed")]
    ProofValidationError,
}
