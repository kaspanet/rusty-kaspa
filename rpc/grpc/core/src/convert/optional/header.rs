use crate::protowire;
use crate::{from, try_from};
use kaspa_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, ToRpcHex};
use std::{convert::TryFrom, str::FromStr};

fn compressed_parents_to_protowire(parents: &kaspa_rpc_core::RpcCompressedParents) -> Vec<protowire::RpcBlockLevelRun> {
    parents
        .raw()
        .iter()
        .map(|(cumulative_level, hashes)| protowire::RpcBlockLevelRun {
            cumulative_level: *cumulative_level as u32,
            parent_hashes: hashes.iter().map(|h| h.to_string()).collect(),
        })
        .collect()
}

fn compressed_parents_from_protowire(runs: &[protowire::RpcBlockLevelRun]) -> RpcResult<kaspa_rpc_core::RpcCompressedParents> {
    let mut tuples = Vec::with_capacity(runs.len());
    for run in runs.iter() {
        let cumulative_level = u8::try_from(run.cumulative_level)?;
        let parents = run.parent_hashes.iter().map(|h| RpcHash::from_str(h).map_err(RpcError::from)).collect::<RpcResult<Vec<_>>>()?;
        tuples.push((cumulative_level, parents));
    }
    Ok(tuples.try_into()?)
}

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcOptionalHeader, protowire::RpcOptionalHeader, {
    Self {
        version: item.version.map(|x| x.into()),
        hash: item.hash.map(|x| x.to_string()),
        parents_by_level: item.parents_by_level.as_ref().map(compressed_parents_to_protowire).unwrap_or_default(),
        hash_merkle_root: item.hash_merkle_root.map(|x| x.to_string()),
        accepted_id_merkle_root: item.accepted_id_merkle_root.map(|x| x.to_string()),
        utxo_commitment: item.utxo_commitment.map(|x| x.to_string()),
        timestamp: item.timestamp.map(|x| x as i64),
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: item.blue_work.map(|x| x.to_rpc_hex()),
        blue_score: item.blue_score,
        pruning_point: item.pruning_point.map(|x| x.to_string()),
    }
});

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcOptionalHeader, kaspa_rpc_core::RpcOptionalHeader, {
    Self {
        version: item.version.map(u16::try_from).transpose()?,
        hash: item.hash.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        parents_by_level: Some(compressed_parents_from_protowire(&item.parents_by_level)?),
        hash_merkle_root: item.hash_merkle_root.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        accepted_id_merkle_root: item.accepted_id_merkle_root.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        utxo_commitment: item.utxo_commitment.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
        timestamp: item.timestamp.map(u64::try_from).transpose()?,
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: item.blue_work.as_ref().map(|x| kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(x)).transpose()?,
        blue_score: item.blue_score,
        pruning_point: item.pruning_point.as_ref().map(|x| RpcHash::from_str(x)).transpose()?,
    }
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_optional_header_fields() {
        let wire = protowire::RpcOptionalHeader {
            version: Some(u16::MAX as u32 + 1),
            ..Default::default()
        };
        assert!(matches!(
            kaspa_rpc_core::RpcOptionalHeader::try_from(&wire),
            Err(RpcError::IntConversionError(_))
        ));

        let wire = protowire::RpcOptionalHeader { timestamp: Some(-1), ..Default::default() };
        assert!(matches!(
            kaspa_rpc_core::RpcOptionalHeader::try_from(&wire),
            Err(RpcError::IntConversionError(_))
        ));
    }
}
