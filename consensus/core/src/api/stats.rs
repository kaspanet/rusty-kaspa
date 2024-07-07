use serde::{Deserialize, Serialize};
use workflow_serializer::prelude::*;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlockCount {
    pub header_count: u64,
    pub block_count: u64,
}

impl BlockCount {
    pub fn new(block_count: u64, header_count: u64) -> Self {
        Self { block_count, header_count }
    }
}

impl Serializer for BlockCount {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.header_count, writer)?;
        store!(u64, &self.block_count, writer)?;

        Ok(())
    }
}

impl Deserializer for BlockCount {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let header_count = load!(u64, reader)?;
        let block_count = load!(u64, reader)?;

        Ok(Self { header_count, block_count })
    }
}

#[derive(Clone, Default)]
pub struct VirtualStateStats {
    /// Number of direct parents of virtual
    pub num_parents: u32,
    pub daa_score: u64,
    pub bits: u32,
    pub past_median_time: u64,
}

pub struct ConsensusStats {
    /// Block and header counts
    pub block_counts: BlockCount,

    /// Overall number of current DAG tips
    pub num_tips: u64,

    /// Virtual-related stats
    pub virtual_stats: VirtualStateStats,
}
