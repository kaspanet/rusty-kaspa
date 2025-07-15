//! 

pub mod simple_token;

pub use simple_token::*;

#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    #[error("Invalid transaction structure")]
    InvalidTransaction,
    
    #[error("Insufficient balance")]
    InsufficientBalance,
    
    #[error("Contract execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Invalid contract code")]
    InvalidCode,
}

pub type ContractResult<T> = Result<T, ContractError>;
