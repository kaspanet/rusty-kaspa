mod error;
use crate::zk_precompiles::{error::ZkIntegrityError, risc0::{groth16::Groth16Receipt, succinct::SuccinctReceipt}};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_txscript_errors::TxScriptError;

mod risc0;

pub trait ZkIntegrityVerifier {
    fn verify_integrity(&self) -> Result<(), ZkIntegrityError>;
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum ZkPrecompile {
    R0Groth16(Groth16Receipt),
    R0Succinct(SuccinctReceipt),
}

impl ZkPrecompile {
    /// Deserialize from raw bytes with a discriminant tag
    ///
    /// Format: [tag: u8][data: remaining bytes]
    /// - tag 0x00: R0Groth16
    /// - tag 0x01: R0Succinct
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, kaspa_txscript_errors::TxScriptError> {
        let tag = bytes[0];
        let data = &bytes[1..];

        match tag {
            0x00 => {
                let receipt = Groth16Receipt::try_from_slice(data).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
                Ok(ZkPrecompile::R0Groth16(receipt))
            }
            0x01 => {
                let receipt = SuccinctReceipt::try_from_slice(data).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
                Ok(ZkPrecompile::R0Succinct(receipt))
            }
            _ => Err(TxScriptError::ZkIntegrity(format!("Unknown ZkPrecompile tag: {}", tag))),
        }
    }
}

impl ZkPrecompile {
    pub fn verify_integrity(&self) -> Result<(), TxScriptError> {
        match self {
            ZkPrecompile::R0Groth16(receipt) => receipt.verify_integrity().map_err(|e| TxScriptError::ZkIntegrity(e.to_string())),
            ZkPrecompile::R0Succinct(receipt) =>  receipt.verify_integrity().map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))
        }
    }
}
