use crate::pb as protowire;
use kaspa_consensus_core::{BlueWorkType, header::Header};
use kaspa_hashes::Hash;

use super::error::ConversionError;
use super::option::TryIntoOptionEx;

// ----------------------------------------------------------------------------
// consensus_core to protowire
// ----------------------------------------------------------------------------

impl From<&Header> for protowire::BlockHeader {
    fn from(item: &Header) -> Self {
        Self {
            version: item.version.into(),
            parents: item
                .parents_by_level
                .raw()
                .iter()
                .map(|(cum, hashes)| protowire::BlockLevelParents {
                    cumulative_level: (*cum).into(),
                    parent_hashes: hashes.iter().map(|h| h.into()).collect(),
                })
                .collect(),
            hash_merkle_root: Some(item.hash_merkle_root.into()),
            accepted_id_merkle_root: Some(item.accepted_id_merkle_root.into()),
            utxo_commitment: Some(item.utxo_commitment.into()),
            timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            // We follow the golang specification of variable big-endian here
            blue_work: item.blue_work.to_be_bytes_var(),
            blue_score: item.blue_score,
            pruning_point: Some(item.pruning_point.into()),
        }
    }
}

// ----------------------------------------------------------------------------
// protowire to consensus_core
// ----------------------------------------------------------------------------

impl TryFrom<protowire::BlockHeader> for Header {
    type Error = ConversionError;
    fn try_from(item: protowire::BlockHeader) -> Result<Self, Self::Error> {
        let parents_by_level = item
            .parents
            .into_iter()
            .map(|p| {
                let cum = u8::try_from(p.cumulative_level)?;
                let parents = p.parent_hashes.into_iter().map(Hash::try_from).collect::<Result<_, _>>()?;
                Ok((cum, parents))
            })
            .collect::<Result<Vec<(u8, Vec<Hash>)>, ConversionError>>()?
            .try_into()?;

        Ok(Header::new_finalized(
            item.version.try_into()?,
            parents_by_level,
            item.hash_merkle_root.try_into_ex()?,
            item.accepted_id_merkle_root.try_into_ex()?,
            item.utxo_commitment.try_into_ex()?,
            item.timestamp.try_into()?,
            item.bits,
            item.nonce,
            item.daa_score,
            // We follow the golang specification of variable big-endian here
            BlueWorkType::from_be_bytes_var(&item.blue_work)?,
            item.blue_score,
            item.pruning_point.try_into_ex()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_parents_wire_roundtrip() {
        let first_run = vec![1.into(), 2.into()];
        let second_run = vec![3.into()];
        let third_run = vec![4.into(), 5.into()];
        let parents_by_level =
            vec![first_run.clone(), first_run.clone(), first_run, second_run.clone(), second_run, third_run].try_into().unwrap();
        let header = Header::new_finalized(
            2,
            parents_by_level,
            Default::default(),
            Default::default(),
            Default::default(),
            1,
            2,
            3,
            4,
            5.into(),
            6,
            Default::default(),
        );

        let wire: protowire::BlockHeader = (&header).into();
        assert_eq!(wire.parents.iter().map(|run| run.cumulative_level).collect::<Vec<_>>(), vec![3, 5, 6]);

        let decoded: Header = wire.try_into().unwrap();
        assert_eq!(decoded.parents_by_level, header.parents_by_level);
        assert_eq!(decoded.hash, header.hash);
    }
}
