use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

use crate::RpcVerbosityTiers;

#[derive(Clone, Debug, Serialize, Deserialize, Default, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcOptionalizedHeaderVerbosity {
    // "NONE": nothing is included
    // "LOW": only the hash is included
    // "MEDIUM": low verbosity + daa score, blue score, and timestamp
    // "HIGH": medium verbosity + pruning point, blue work and version
    // "FULL": all fields, except for parents by level, are included.
    pub verbosity: RpcVerbosityTiers,
    pub include_parents_by_level: bool,
    //TODO:
    // Note: this should override all other selections with a custom bitflag selector
    //pub bitflags: Option<u32>
}

impl RpcOptionalizedHeaderVerbosity {
    pub fn new(verbosity: RpcVerbosityTiers, include_parents_by_level: bool) -> Self {
        Self { verbosity, include_parents_by_level }
    }

    pub fn is_empty(&self) -> bool {
        self.verbosity.is_none() && !self.include_parents_by_level()
    }

    pub fn include_hash(&self) -> bool {
        self.verbosity.is_low_or_higher()
    }

    pub fn include_version(&self) -> bool {
        self.verbosity.is_high_or_higher()
    }

    pub fn include_parents_by_level(&self) -> bool {
        self.include_parents_by_level
    }

    pub fn include_hash_merkle_root(&self) -> bool {
        self.verbosity.is_full()
    }

    pub fn include_accepted_id_merkle_root(&self) -> bool {
        self.verbosity.is_full()
    }

    pub fn include_utxo_commitment(&self) -> bool {
        self.verbosity.is_full()
    }

    pub fn include_timestamp(&self) -> bool {
        self.verbosity.is_medium_or_higher()
    }

    pub fn include_bits(&self) -> bool {
        self.verbosity.is_full()
    }

    pub fn include_nonce(&self) -> bool {
        self.verbosity.is_full()
    }

    pub fn include_daa_score(&self) -> bool {
        self.verbosity.is_medium_or_higher()
    }

    pub fn include_blue_work(&self) -> bool {
        self.verbosity.is_high_or_higher()
    }

    pub fn include_blue_score(&self) -> bool {
        self.verbosity.is_medium_or_higher()
    }

    pub fn include_pruning_point(&self) -> bool {
        self.verbosity.is_high_or_higher()
    }
}

impl Serializer for RpcOptionalizedHeaderVerbosity {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcVerbosityTiers, &self.verbosity, writer)?;
        store!(bool, &self.include_parents_by_level, writer)?;
        Ok(())
    }
}

impl Deserializer for RpcOptionalizedHeaderVerbosity {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let verbosity = load!(RpcVerbosityTiers, reader)?;
        let include_parents_by_level = load!(bool, reader)?;
        Ok(Self { verbosity, include_parents_by_level })
    }
}
