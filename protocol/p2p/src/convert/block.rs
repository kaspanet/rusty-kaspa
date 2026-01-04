use super::error::ConversionError;
use super::header::{HeaderFormat, Versioned};
use crate::pb as protowire;
use kaspa_consensus_core::{block::Block, tx::Transaction};
type BlockBody = Vec<Transaction>;
// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<(HeaderFormat, &Block)> for protowire::BlockMessage {
    fn from(value: (HeaderFormat, &Block)) -> Self {
        let (header_format, block) = value;
        Self {
            header: Some((header_format, block.header.as_ref()).into()),
            transactions: block.transactions.iter().map(|tx| tx.into()).collect(),
        }
    }
}
impl From<&BlockBody> for protowire::BlockBodyMessage {
    fn from(block_body: &BlockBody) -> Self {
        Self { transactions: block_body.iter().map(|tx| tx.into()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<Versioned<protowire::BlockMessage>> for Block {
    type Error = ConversionError;

    fn try_from(value: Versioned<protowire::BlockMessage>) -> Result<Self, Self::Error> {
        let Versioned(header_format, block) = value;
        let header = block.header.ok_or(ConversionError::NoneValue)?;
        Ok(Self::new(
            Versioned(header_format, header).try_into()?,
            block.transactions.into_iter().map(|i| i.try_into()).collect::<Result<Vec<Transaction>, Self::Error>>()?,
        ))
    }
}

impl TryFrom<protowire::BlockBodyMessage> for BlockBody {
    type Error = ConversionError;
    fn try_from(body_message: protowire::BlockBodyMessage) -> Result<Self, Self::Error> {
        let blk_body: BlockBody =
            body_message.transactions.into_iter().map(|i| i.try_into()).collect::<Result<Vec<Transaction>, ConversionError>>()?;
        Ok(blk_body)
    }
}
