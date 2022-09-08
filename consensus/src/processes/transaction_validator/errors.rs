use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum TxRuleError {
    #[error("transaction has no inputs")]
    NoTxInputs,

    #[error("transaction has duplicate inputs")]
    TxDuplicateInputs,

    #[error("transaction has non zero gas value")]
    TxHasGas,

    #[error("a non coinbase transaction has a paylaod")]
    NonCoinbaseTxHasPayload,

    #[error("transaction version {0} is unknown")]
    UnknownTxVersion(u16),

    #[error("transaction has {0} inputs where the max allowed is {1}")]
    TooManyInputs(usize, usize),

    #[error("transaction input #{0} signature script is above {1} bytes")]
    TooBigSignatureScript(usize, usize),

    #[error("transaction input #{0} signature script is above {1} bytes")]
    TooBigScriptPublicKey(usize, usize),
}

pub type TxResult<T> = std::result::Result<T, TxRuleError>;
