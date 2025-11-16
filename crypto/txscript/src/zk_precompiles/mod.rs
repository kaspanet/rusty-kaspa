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
    pub fn from_bytes(data: &[u8],tag:u8) -> Result<Self, kaspa_txscript_errors::TxScriptError> {
        match tag {
            0x20 => {
                let receipt: Groth16Receipt = borsh::from_slice(data).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
                Ok(ZkPrecompile::R0Groth16(receipt))
            }
            0x21 => {
                let receipt: SuccinctReceipt =  borsh::from_slice(data).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))?;
                Ok(ZkPrecompile::R0Succinct(receipt))
            }
            _ =>{
                Err(TxScriptError::ZkIntegrity(format!("Unknown ZkPrecompile tag: {}", tag)))
            } 
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

pub fn compute_zk_sigop_cost(tag:u8) -> u8 {
    // Match first byte to determine type
    match tag {
        0x20 => 5,
        0x21 => 10,
        _ => 20,
    }
}