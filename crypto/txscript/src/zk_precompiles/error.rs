use kaspa_txscript_errors::TxScriptError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZkIntegrityError {
    //#[error("Groth16 error: {0}")]
   //Groth16(#[from] crate::zk_precompiles::groth16::Groth16Error),
    #[error("R0 error: {0}")]
    R0Error(#[from] crate::zk_precompiles::risc0::R0Error),
    #[error("Txscript error: {0}")]
    TxScript(#[from] TxScriptError),
    #[error("Unknown tag: {0}")]
    UnknownTag(u8),
}
