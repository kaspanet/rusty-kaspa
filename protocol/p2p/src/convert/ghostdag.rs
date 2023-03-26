use super::{error::ConversionError, option::TryIntoOptionEx};
use crate::pb as protowire;
use kaspa_consensus_core::{
    trusted::{ExternalGhostdagData, TrustedGhostdagData, TrustedHeader},
    BlockHashMap, BlueWorkType, HashMapCustomHasher, KType,
};
use kaspa_hashes::Hash;
use std::sync::Arc;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&ExternalGhostdagData> for protowire::GhostdagData {
    fn from(item: &ExternalGhostdagData) -> Self {
        Self {
            blue_score: item.blue_score,
            blue_work: item.blue_work.to_be_bytes_var(),
            selected_parent: Some(item.selected_parent.into()),
            merge_set_blues: item.mergeset_blues.iter().map(|h| h.into()).collect(),
            merge_set_reds: item.mergeset_reds.iter().map(|h| h.into()).collect(),
            blues_anticone_sizes: item
                .blues_anticone_sizes
                .iter()
                .map(|(h, &s)| protowire::BluesAnticoneSizes { blue_hash: Some(h.into()), anticone_size: s.into() })
                .collect(),
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::BluesAnticoneSizes> for (Hash, KType) {
    type Error = ConversionError;
    fn try_from(item: protowire::BluesAnticoneSizes) -> Result<Self, Self::Error> {
        Ok((item.blue_hash.try_into_ex()?, item.anticone_size.try_into()?))
    }
}

impl TryFrom<protowire::GhostdagData> for ExternalGhostdagData {
    type Error = ConversionError;
    fn try_from(item: protowire::GhostdagData) -> Result<Self, Self::Error> {
        let mut blues_anticone_sizes = BlockHashMap::<KType>::with_capacity(item.blues_anticone_sizes.len());
        for res in item.blues_anticone_sizes.into_iter().map(<(Hash, KType)>::try_from) {
            let (k, v) = res?;
            blues_anticone_sizes.insert(k, v);
        }
        Ok(Self {
            blue_score: item.blue_score,
            blue_work: BlueWorkType::from_be_bytes_var(&item.blue_work)?,
            selected_parent: item.selected_parent.try_into_ex()?,
            mergeset_blues: item.merge_set_blues.into_iter().map(Hash::try_from).collect::<Result<Vec<Hash>, ConversionError>>()?,
            mergeset_reds: item.merge_set_reds.into_iter().map(Hash::try_from).collect::<Result<Vec<Hash>, ConversionError>>()?,
            blues_anticone_sizes,
        })
    }
}

impl TryFrom<protowire::BlockGhostdagDataHashPair> for TrustedGhostdagData {
    type Error = ConversionError;
    fn try_from(pair: protowire::BlockGhostdagDataHashPair) -> Result<Self, Self::Error> {
        Ok(Self::new(pair.hash.try_into_ex()?, pair.ghostdag_data.try_into_ex()?))
    }
}

impl TryFrom<protowire::DaaBlockV4> for TrustedHeader {
    type Error = ConversionError;
    fn try_from(b: protowire::DaaBlockV4) -> Result<Self, Self::Error> {
        Ok(Self::new(b.header.try_into_ex().map(Arc::new)?, b.ghostdag_data.try_into_ex()?))
    }
}
