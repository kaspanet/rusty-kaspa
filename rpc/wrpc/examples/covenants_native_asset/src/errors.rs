use kaspa_txscript::script_builder::ScriptBuilderError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CovenantError {
    #[error("invalid payload length: expected {expected}, got {actual}")]
    InvalidPayloadLength { expected: usize, actual: usize },
    #[error("invalid payload magic")]
    InvalidPayloadMagic,
    #[error("unsupported payload op: {value}")]
    InvalidPayloadOp { value: u8 },
    #[error("invalid field length for {0}")]
    InvalidField(&'static str),
    #[error("invalid script number encoding for {0}")]
    InvalidScriptNum(&'static str),
    #[error("script number mismatch for {field}: encoded {encoded}, expected {expected}")]
    ScriptNumMismatch { field: &'static str, encoded: u64, expected: u64 },
    #[error("script number for {field} exceeds i64 range: {value}")]
    ScriptNumOverflow { field: &'static str, value: u64 },
    #[error("payload length {payload_len} exceeds preimage length {preimage_len}")]
    PayloadLargerThanPreimage { payload_len: usize, preimage_len: usize },
    #[error("grandparent tx missing output 0")]
    MissingGrandparentOutput0,
    #[error("grandparent preimage length mismatch: expected {expected_len}, got {actual_len}")]
    GrandparentPreimageLengthMismatch { expected_len: usize, actual_len: usize },
    #[error("grandparent output0 script length mismatch: expected {expected_len}, got {actual_len}")]
    GrandparentOutputScriptLenMismatch { expected_len: u64, actual_len: u64 },
    #[error("spk length out of range for {field}: expected {min}-{max}, got {actual}")]
    SpkBytesLengthOutOfRange { field: &'static str, min: usize, max: usize, actual: usize },
    #[error("amount {amount} exceeds remaining supply {remaining}")]
    AmountExceedsRemainingSupply { remaining: u64, amount: u64 },
    #[error("insufficient funds: available {available}, required {required}")]
    InsufficientFunds { available: u64, required: u64 },
    #[error(transparent)]
    ScriptBuilder(#[from] ScriptBuilderError),
}
