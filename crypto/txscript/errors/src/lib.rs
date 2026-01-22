use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Error, PartialEq, Eq, Debug, Clone)]
pub enum TxScriptError {
    #[error("invalid opcode length: {0:02x?}")]
    MalformedPushSize(Vec<u8>),
    #[error("opcode requires {0} bytes, but script only has {1} remaining")]
    MalformedPush(usize, usize),
    #[error("transaction input {0} is out of bounds, should be non-negative below {1}")]
    InvalidInputIndex(i32, usize),
    #[error("transaction input {0} is either out of bounds or has no associated covenant outputs")]
    InvalidCovInputIndex(i32),
    #[error("combined stack size {0} > max allowed {1}")]
    StackSizeExceeded(usize, usize),
    #[error("attempt to execute invalid opcode {0}")]
    InvalidOpcode(String),
    #[error("attempt to execute reserved opcode {0}")]
    OpcodeReserved(String),
    #[error("attempt to execute disabled opcode {0}")]
    OpcodeDisabled(String),
    #[error("attempt to read from empty stack")]
    EmptyStack,
    #[error("stack contains {0} unexpected items")]
    CleanStack(usize),
    // We return error if stack entry is false
    #[error("false stack entry at end of script execution")]
    EvalFalse,
    #[error("script returned early")]
    EarlyReturn,
    #[error("script ran, but verification failed")]
    VerifyError,
    #[error("encountered invalid state while running script: {0}")]
    InvalidState(String),
    #[error("signature invalid: {0}")]
    InvalidSignature(secp256k1::Error),
    #[error("invalid signature in sig cache")]
    SigcacheSignatureInvalid,
    #[error("exceeded max operation limit of {0}")]
    TooManyOperations(i32),
    #[error("Engine is not running on a transaction input")]
    NotATransactionInput,
    #[error("element size {0} exceeds max allowed size {1}")]
    ElementTooBig(usize, usize),
    #[error("push encoding is not minimal: {0}")]
    NotMinimalData(String),
    #[error("opcode not supported on current source: {0}")]
    InvalidSource(String),
    #[error("Unsatisfied lock time: {0}")]
    UnsatisfiedLockTime(String),
    #[error("Number too big: {0}")]
    NumberTooBig(String),
    #[error("not all signatures empty on failed checkmultisig")]
    NullFail,
    #[error("invalid signature count: {0}")]
    InvalidSignatureCount(String),
    #[error("invalid pubkey count: {0}")]
    InvalidPubKeyCount(String),
    #[error("invalid hash type {0:#04x}")]
    InvalidSigHashType(u8),
    #[error("unsupported public key type")]
    PubKeyFormat,
    #[error("invalid signature length {0}")]
    SigLength(usize),
    #[error("no scripts to run")]
    NoScripts,
    #[error("signature script is not push only")]
    SignatureScriptNotPushOnly,
    #[error("end of script reached in conditional execution")]
    ErrUnbalancedConditional,
    #[error("opcode requires at least {0} but stack has only {1}")]
    InvalidStackOperation(usize, usize),
    #[error("script of size {0} exceeded maximum allowed size of {1}")]
    ScriptSize(usize, usize),
    #[error("transaction output {0} is out of bounds, should be non-negative below {1}")]
    InvalidOutputIndex(i32, usize),
    #[error(transparent)]
    Serialization(#[from] SerializationError),
    #[error("sig op count exceeds passed limit of {0}")]
    ExceededSigOpLimit(u8),
    #[error("substring [{0}:{1}] is out of bounds for string of length {2}")]
    OutOfBoundsSubstring(usize, usize, usize),
    #[error("{0} cannot be used as an array index")]
    InvalidIndex(i32),

    #[error("{0} is not a valid covenant output index for input {1} with {2} covenant outputs")]
    InvalidInputCovOutIndex(usize, usize, usize),
    #[error("blockhash must be exactly 32 bytes long, got {0} bytes instead")]
    InvalidLengthOfBlockHash(usize),
    #[error("block {0} not selected")]
    BlockNotSelected(String),
    #[error("block {0} already pruned")]
    BlockAlreadyPruned(String),
    #[error("block {0} is too deep")]
    BlockIsTooDeep(String),
    #[error("covenants error: {0}")]
    CovenantsError(#[from] CovenantsError),
}

#[derive(Error, PartialEq, Eq, Debug, Clone, Copy)]
pub enum SerializationError {
    #[error("Number exceeds {1} bytes: {0}")]
    NumberTooLong(i64, usize),
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CovenantsError {
    #[error("output #{0} covenant id does not correspond to the authorizing input covenant id")]
    WrongCovenantId(usize),
    #[error("output #{0} covenant id does not correspond to the authorizing input outpoint hashing (genesis case)")]
    WrongGenesisCovenantId(usize),
    #[error("output #{0} covenant authorizing input index {1} is out of bounds")]
    AuthInputOutOfBounds(usize, u16),
    #[error("covenant id {0} input {1} is out of bounds")]
    InvalidCovInIndex(Hash, usize),
    #[error("covenant id {0} output {1} is out of bounds")]
    InvalidCovOutIndex(Hash, usize),
}
