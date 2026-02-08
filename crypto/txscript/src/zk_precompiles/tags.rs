use crate::zk_precompiles::error::ZkIntegrityError;

#[derive(Copy, Clone)]
#[repr(u8)]
/// The supported ZK proof tags
pub enum ZkTag {
    Groth16 = 0x20,
    R0Succinct = 0x21,
}

impl TryFrom<u8> for ZkTag {
    type Error = ZkIntegrityError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x20 => Ok(ZkTag::Groth16),
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
    pub fn sigop_cost(&self) -> u16 {
        match self {
            ZkTag::Groth16 => 140,
            ZkTag::R0Succinct => 250,
        }
    }

    pub fn max_cost() -> u16 {
        250 // The highest cost among supported tags
    }
}

#[cfg(test)]
mod tests {
    use super::ZkTag;

    fn expected_max_cost() -> u16 {
        let mut max_cost = 0;

        for tag in [ZkTag::Groth16, ZkTag::R0Succinct] {
            // Intentionally exhaustive match so adding a new enum variant
            // fails to compile until this list is updated.
            let cost = match tag {
                ZkTag::Groth16 => ZkTag::Groth16.sigop_cost(),
                ZkTag::R0Succinct => ZkTag::R0Succinct.sigop_cost(),
            };

            if cost > max_cost {
                max_cost = cost;
            }
        }

        max_cost
    }

    #[test]
    fn max_cost_matches_hardcoded_value() {
        assert_eq!(ZkTag::max_cost(), expected_max_cost());
    }
}
