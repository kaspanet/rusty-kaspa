use crate::constants::MAX_SOMPI;
use crate::tx::TransactionOutpoint;
use kaspa_txscript_errors::TxScriptError;
use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum TxRuleError {
    #[error("transaction has no inputs")]
    NoTxInputs,

    #[error("transaction has duplicate inputs")]
    TxDuplicateInputs,

    #[error("transaction has non zero gas value")]
    TxHasGas,

    #[error("a non coinbase transaction has a payload")]
    NonCoinbaseTxHasPayload,

    #[error("transaction version {0} is unknown")]
    UnknownTxVersion(u16),

    #[error("transaction has {0} inputs where the max allowed is {1}")]
    TooManyInputs(usize, usize),

    #[error("transaction has {0} outputs where the max allowed is {1}")]
    TooManyOutputs(usize, usize),

    #[error("transaction input #{0} signature script is above {1} bytes")]
    TooBigSignatureScript(usize, usize),

    #[error("transaction input #{0} signature script is above {1} bytes")]
    TooBigScriptPublicKey(usize, usize),

    #[error("transaction input #{0} is not finalized")]
    NotFinalized(usize),

    #[error("coinbase transaction has {0} inputs while none are expected")]
    CoinbaseHasInputs(usize),

    #[error("coinbase transaction has {0} outputs while at most {1} are expected")]
    CoinbaseTooManyOutputs(usize, u64),

    #[error("script public key of coinbase output #{0} is too long")]
    CoinbaseScriptPublicKeyTooLong(usize),

    #[error(
        "transaction input #{0} tried to spend coinbase outpoint {1} with daa score of {2} 
    while the merging block daa score is {3} and the coinbase maturity period of {4} hasn't passed yet"
    )]
    ImmatureCoinbaseSpend(usize, TransactionOutpoint, u64, u64, u64),

    #[error("transaction total inputs spending amount overflowed u64")]
    InputAmountOverflow,

    #[error("transaction total inputs spending amount is higher than the max allowed of {}", MAX_SOMPI)]
    InputAmountTooHigh,

    #[error("transaction output {0} has zero value")]
    TxOutZero(usize),

    #[error("transaction output {0} value is higher than the max allowed of {}", MAX_SOMPI)]
    TxOutTooHigh(usize),

    #[error("transaction total outputs value overflowed u64")]
    OutputsValueOverflow,

    #[error("transaction total outputs value is higher than the max allowed of {}", MAX_SOMPI)]
    TotalTxOutTooHigh,

    #[error("transaction tries to spend {0} while its total inputs amount is {1}")]
    SpendTooHigh(u64, u64),

    #[error("one of the transaction sequence locks conditions was not met")]
    SequenceLockConditionsAreNotMet,

    #[error("outpoints corresponding to some transaction inputs are missing from current utxo context")]
    MissingTxOutpoints,

    #[error("failed to verify the signature script: {0}")]
    SignatureInvalid(TxScriptError),

    #[error("input {0} sig op count is {1}, but the calculated value is {2}")]
    WrongSigOpCount(usize, u64, u64),

    #[error("contextual mass (including storage mass) is incomputable")]
    MassIncomputable,

    #[error("calculated contextual mass (including storage mass) {0} is not equal to the committed mass field {1}")]
    WrongMass(u64, u64),
}

pub type TxResult<T> = std::result::Result<T, TxRuleError>;
