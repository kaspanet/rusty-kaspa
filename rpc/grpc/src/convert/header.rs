use crate::protowire;
use rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

impl From<&rpc_core::RpcHeader> for protowire::RpcBlockHeader {
    fn from(item: &rpc_core::RpcHeader) -> Self {
        Self {
            version: item.version.into(),
            parents: item.parents_by_level.iter().map(protowire::RpcBlockLevelParents::from).collect(),
            hash_merkle_root: item.hash_merkle_root.to_string(),
            accepted_id_merkle_root: item.accepted_id_merkle_root.to_string(),
            utxo_commitment: item.utxo_commitment.to_string(),
            timestamp: item.timestamp.try_into().expect("timestamp is always convertible to i64"),
            bits: item.bits,
            nonce: item.nonce,
            daa_score: item.daa_score,
            blue_work: item.blue_work.to_rpc_hex(),
            blue_score: item.blue_score,
            pruning_point: item.pruning_point.to_string(),
        }
    }
}

impl From<&Vec<RpcHash>> for protowire::RpcBlockLevelParents {
    fn from(item: &Vec<RpcHash>) -> Self {
        Self { parent_hashes: item.iter().map(|x| x.to_string()).collect() }
    }
}

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

impl TryFrom<&protowire::RpcBlockHeader> for rpc_core::RpcHeader {
    type Error = RpcError;
    fn try_from(item: &protowire::RpcBlockHeader) -> RpcResult<Self> {
        Ok(Self::new(
            item.version.try_into()?,
            item.parents.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?,
            RpcHash::from_str(&item.hash_merkle_root)?,
            RpcHash::from_str(&item.accepted_id_merkle_root)?,
            RpcHash::from_str(&item.utxo_commitment)?,
            item.timestamp.try_into()?,
            item.bits,
            item.nonce,
            item.daa_score,
            rpc_core::RpcBlueWorkType::from_rpc_hex(&item.blue_work)?,
            item.blue_score,
            RpcHash::from_str(&item.pruning_point)?,
        ))
    }
}

impl TryFrom<&protowire::RpcBlockLevelParents> for Vec<RpcHash> {
    type Error = RpcError;
    fn try_from(item: &protowire::RpcBlockLevelParents) -> RpcResult<Self> {
        Ok(item.parent_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<rpc_core::RpcHash>, faster_hex::Error>>()?)
    }
}

#[cfg(test)]
mod tests {
    use crate::protowire;
    use rpc_core::{RpcHash, RpcHeader};

    fn new_unique() -> RpcHash {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        RpcHash::from_u64_word(c)
    }

    fn test_parents_by_level_rxr(r: &Vec<Vec<RpcHash>>, r2: &[Vec<RpcHash>]) {
        for i in 0..r.len() {
            for j in 0..r[i].len() {
                assert_eq!(r[i][j], r2[i][j]);
            }
        }
    }
    fn test_parents_by_level_rxp(r: &Vec<Vec<RpcHash>>, p: &[protowire::RpcBlockLevelParents]) {
        for i in 0..r.len() {
            for j in 0..r[i].len() {
                assert_eq!(r[i][j].to_string(), p[i].parent_hashes[j]);
            }
        }
    }

    #[test]
    fn test_rpc_block_level_parents() {
        let p = protowire::RpcBlockLevelParents {
            parent_hashes: vec![new_unique().to_string(), new_unique().to_string(), new_unique().to_string()],
        };
        let r: Vec<RpcHash> = (&p).try_into().unwrap();
        let p2: protowire::RpcBlockLevelParents = (&r).into();
        for (i, _) in r.iter().enumerate() {
            assert_eq!(p.parent_hashes[i], r[i].to_string());
            assert_eq!(p2.parent_hashes[i], r[i].to_string());
            assert_eq!(p.parent_hashes[i], p2.parent_hashes[i]);
        }
        assert_eq!(p, p2);

        let r: Vec<RpcHash> = vec![new_unique(), new_unique()];
        let p: protowire::RpcBlockLevelParents = (&r).into();
        let r2: Vec<RpcHash> = (&p).try_into().unwrap();
        for i in 0..r.len() {
            assert_eq!(p.parent_hashes[i], r[i].to_string());
            assert_eq!(p.parent_hashes[i], r2[i].to_string());
            assert_eq!(r[i], r2[i]);
        }
        assert_eq!(r, r2);
    }

    #[test]
    fn test_rpc_header() {
        let r = RpcHeader::new(
            0,
            vec![vec![new_unique(), new_unique(), new_unique()], vec![new_unique()], vec![new_unique(), new_unique()]],
            new_unique(),
            new_unique(),
            new_unique(),
            123,
            12345,
            98765,
            120055,
            459912.into(),
            1928374,
            new_unique(),
        );
        let p: protowire::RpcBlockHeader = (&r).into();
        let r2: RpcHeader = (&p).try_into().unwrap();
        let p2: protowire::RpcBlockHeader = (&r2).into();

        assert_eq!(r.parents_by_level, r2.parents_by_level);
        assert_eq!(p.parents, p2.parents);
        test_parents_by_level_rxr(&r.parents_by_level, &r2.parents_by_level);
        test_parents_by_level_rxp(&r.parents_by_level, &p.parents);
        test_parents_by_level_rxp(&r.parents_by_level, &p2.parents);
        test_parents_by_level_rxp(&r2.parents_by_level, &p2.parents);

        assert_eq!(r.hash, r2.hash);
        assert_eq!(p, p2);
    }
}
