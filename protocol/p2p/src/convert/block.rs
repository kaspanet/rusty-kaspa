use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use consensus_core::block::Block;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Block> for protowire::BlockMessage {
    fn from(block: &Block) -> Self {
        // TODO: txs
        Self { header: Some(block.header.as_ref().into()), transactions: vec![] }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::BlockMessage> for Block {
    type Error = ConversionError;

    fn try_from(block: &protowire::BlockMessage) -> Result<Self, Self::Error> {
        Ok(Self::new((&block.header).try_into_ex()?, vec![])) // TODO: txs
    }
}
