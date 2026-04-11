use kaspa_txscript_errors::TxScriptError;
use risc0_zkvm::PrunedValueError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZkIntegrityError {
    #[error("Groth16 error: {0}")]
    Groth16(#[from] crate::zk_precompiles::groth16::Groth16Error),
    #[error("R0 error: {0}")]
    R0Error(#[from] crate::zk_precompiles::risc0::R0Error),
    #[error("Txscript error: {0}")]
    TxScript(#[from] TxScriptError),
    #[error("Script builder error: {0}")]
    ScriptBuilder(#[from] crate::script_builder::ScriptBuilderError),
    #[error("Pruned value error: {0}")]
    PrunedValue(#[from] PrunedValueError),
    
    #[error("Unknown tag: {0}")]
    UnknownTag(u8),
}
