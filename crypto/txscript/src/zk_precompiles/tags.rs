use crate::zk_precompiles::error::ZkIntegrityError;

#[repr(u8)]
/// The supported ZK proof tags
pub enum ZkTag {
    R0Groth16 = 0x20,
    R0Succinct = 0x21,
}

impl TryFrom<u8> for ZkTag {
    type Error = ZkIntegrityError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x20 => Ok(ZkTag::R0Groth16),
            0x21 => Ok(ZkTag::R0Succinct),
            _ => Err(ZkIntegrityError::UnknownTag(value)),
        }
    }
}

impl ZkTag {
    /// Returns the sigop cost associated with the ZK tag
    /// Prices are based on benchmarks and estimations of verification complexity
    /// 
    /// Since 1 sigop is priced at 1000 gram, the costs are in 1000 gram units
    pub fn sigop_cost(&self) -> u32 {
        match self {
            ZkTag::R0Groth16 => 135,
            ZkTag::R0Succinct => 740,
        }
    }

    pub fn max_cost() -> u32 {
        740 // The highest cost among supported tags
    }
}