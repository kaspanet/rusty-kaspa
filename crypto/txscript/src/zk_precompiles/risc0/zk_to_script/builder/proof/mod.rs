mod groth16;
mod succinct;
use super::super::Result;
pub use groth16::R0_SERIALIZED_UNCOMPRESSED_VK;

/// The final output of the builder, containing both the sig script and redeem script.
pub struct FinalizedR0Script {
    pub sig_script: Vec<u8>,
    pub redeem_script: Vec<u8>,
}
