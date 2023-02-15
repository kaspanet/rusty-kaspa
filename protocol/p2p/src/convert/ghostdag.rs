use super::{error::ConversionError, hash::try_from_hash_op};
use crate::pb::{self as protowire, BluesAnticoneSizes};
use consensus_core::{
    ghostdag::{ExternalGhostdagData, KType},
    BlockHashMap, BlueWorkType, HashMapCustomHasher,
};
use hashes::Hash;
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
                .map(|(h, &s)| BluesAnticoneSizes { blue_hash: Some(h.into()), anticone_size: s.into() })
                .collect(),
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::GhostdagData> for ExternalGhostdagData {
    type Error = ConversionError;
    fn try_from(item: &protowire::GhostdagData) -> Result<Self, Self::Error> {
        let mut blues_anticone_sizes = BlockHashMap::<KType>::with_capacity(item.blues_anticone_sizes.len());
        for res in item.blues_anticone_sizes.iter().map(<(Hash, KType)>::try_from) {
            let (k, v) = res?;
            blues_anticone_sizes.insert(k, v);
        }
        Ok(Self {
            blue_score: item.blue_score,
            blue_work: BlueWorkType::from_be_bytes_var(&item.blue_work)?,
            selected_parent: try_from_hash_op(&item.selected_parent)?,
            mergeset_blues: Arc::new(item.merge_set_blues.iter().map(Hash::try_from).collect::<Result<Vec<Hash>, ConversionError>>()?),
            mergeset_reds: Arc::new(item.merge_set_reds.iter().map(Hash::try_from).collect::<Result<Vec<Hash>, ConversionError>>()?),
            blues_anticone_sizes: Arc::new(blues_anticone_sizes),
        })
    }
}

impl TryFrom<&protowire::BluesAnticoneSizes> for (Hash, KType) {
    type Error = ConversionError;
    fn try_from(item: &protowire::BluesAnticoneSizes) -> Result<Self, Self::Error> {
        Ok((try_from_hash_op(&item.blue_hash)?, item.anticone_size.try_into()?))
    }
}
