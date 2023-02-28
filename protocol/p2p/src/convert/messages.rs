use super::{
    error::ConversionError,
    model::trusted::{TrustedDataEntry, TrustedDataPackage},
    option::TryIntoOptionEx,
};
use crate::pb as protowire;
use consensus_core::{
    header::Header,
    pruning::{PruningPointProof, PruningPointsList},
    tx::{TransactionOutpoint, UtxoEntry},
};
use hashes::Hash;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::RequestHeadersMessage> for (Hash, Hash) {
    type Error = ConversionError;
    fn try_from(msg: protowire::RequestHeadersMessage) -> Result<Self, Self::Error> {
        Ok((msg.high_hash.try_into_ex()?, msg.low_hash.try_into_ex()?))
    }
}

impl TryFrom<protowire::RequestIbdChainBlockLocatorMessage> for (Option<Hash>, Option<Hash>) {
    type Error = ConversionError;
    fn try_from(msg: protowire::RequestIbdChainBlockLocatorMessage) -> Result<Self, Self::Error> {
        let low = match msg.low_hash {
            Some(low) => Some(low.try_into()?),
            None => None,
        };

        let high = match msg.high_hash {
            Some(high) => Some(high.try_into()?),
            None => None,
        };

        Ok((low, high))
    }
}

impl TryFrom<protowire::PruningPointProofMessage> for PruningPointProof {
    type Error = ConversionError;
    fn try_from(msg: protowire::PruningPointProofMessage) -> Result<Self, Self::Error> {
        msg.headers.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::PruningPointsMessage> for PruningPointsList {
    type Error = ConversionError;
    fn try_from(msg: protowire::PruningPointsMessage) -> Result<Self, Self::Error> {
        msg.headers.into_iter().map(|x| x.try_into().map(Arc::new)).collect()
    }
}

impl TryFrom<protowire::TrustedDataMessage> for TrustedDataPackage {
    type Error = ConversionError;
    fn try_from(msg: protowire::TrustedDataMessage) -> Result<Self, Self::Error> {
        Ok(Self::new(
            msg.daa_window.into_iter().map(|x| x.try_into()).collect::<Result<Vec<_>, Self::Error>>()?,
            msg.ghostdag_data.into_iter().map(|x| x.try_into()).collect::<Result<Vec<_>, Self::Error>>()?,
        ))
    }
}

impl TryFrom<protowire::BlockWithTrustedDataV4Message> for TrustedDataEntry {
    type Error = ConversionError;
    fn try_from(msg: protowire::BlockWithTrustedDataV4Message) -> Result<Self, Self::Error> {
        Ok(Self::new(msg.block.try_into_ex()?, msg.daa_window_indices, msg.ghostdag_data_indices))
    }
}

impl TryFrom<protowire::IbdChainBlockLocatorMessage> for Vec<Hash> {
    type Error = ConversionError;
    fn try_from(msg: protowire::IbdChainBlockLocatorMessage) -> Result<Self, Self::Error> {
        msg.block_locator_hashes.into_iter().map(|v| v.try_into()).collect()
    }
}

impl TryFrom<protowire::BlockHeadersMessage> for Vec<Arc<Header>> {
    type Error = ConversionError;
    fn try_from(msg: protowire::BlockHeadersMessage) -> Result<Self, Self::Error> {
        msg.block_headers.into_iter().map(|v| v.try_into().map(Arc::new)).collect()
    }
}

impl TryFrom<protowire::PruningPointUtxoSetChunkMessage> for Vec<(TransactionOutpoint, UtxoEntry)> {
    type Error = ConversionError;

    fn try_from(msg: protowire::PruningPointUtxoSetChunkMessage) -> Result<Self, Self::Error> {
        msg.outpoint_and_utxo_entry_pairs.into_iter().map(|p| p.try_into()).collect()
    }
}

impl TryFrom<protowire::RequestPruningPointUtxoSetMessage> for Hash {
    type Error = ConversionError;

    fn try_from(msg: protowire::RequestPruningPointUtxoSetMessage) -> Result<Self, Self::Error> {
        msg.pruning_point_hash.try_into_ex()
    }
}

impl TryFrom<protowire::InvRelayBlockMessage> for Hash {
    type Error = ConversionError;

    fn try_from(msg: protowire::InvRelayBlockMessage) -> Result<Self, Self::Error> {
        msg.hash.try_into_ex()
    }
}
