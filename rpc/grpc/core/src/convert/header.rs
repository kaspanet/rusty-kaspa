use crate::protowire;
use crate::{from, try_from};
use kaspa_consensus_core::header::Header;
use kaspa_rpc_core::{FromRpcHex, RpcError, RpcHash, RpcResult, ToRpcHex};
use std::str::FromStr;

// ----------------------------------------------------------------------------
// rpc_core to protowire
// ----------------------------------------------------------------------------

from!(item: &kaspa_rpc_core::RpcHeader, protowire::RpcBlockHeader, {
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
});

from!(item: &kaspa_rpc_core::RpcRawHeader, protowire::RpcBlockHeader, {
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
});

from!(item: &Vec<RpcHash>, protowire::RpcBlockLevelParents, { Self { parent_hashes: item.iter().map(|x| x.to_string()).collect() } });

// ----------------------------------------------------------------------------
// protowire to rpc_core
// ----------------------------------------------------------------------------

try_from!(item: &protowire::RpcBlockHeader, kaspa_rpc_core::RpcHeader, {
    // We re-hash the block to remain as most trustless as possible
    let header = Header::new_finalized(
        item.version.try_into()?,
        item.parents.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?,
        RpcHash::from_str(&item.hash_merkle_root)?,
        RpcHash::from_str(&item.accepted_id_merkle_root)?,
        RpcHash::from_str(&item.utxo_commitment)?,
        item.timestamp.try_into()?,
        item.bits,
        item.nonce,
        item.daa_score,
        kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(&item.blue_work)?,
        item.blue_score,
        RpcHash::from_str(&item.pruning_point)?,
    );

    header.into()
});

try_from!(item: &protowire::RpcBlockHeader, kaspa_rpc_core::RpcRawHeader, {
    Self {
        version: item.version.try_into()?,
        parents_by_level: item.parents.iter().map(Vec::<RpcHash>::try_from).collect::<RpcResult<Vec<Vec<RpcHash>>>>()?,
        hash_merkle_root: RpcHash::from_str(&item.hash_merkle_root)?,
        accepted_id_merkle_root: RpcHash::from_str(&item.accepted_id_merkle_root)?,
        utxo_commitment: RpcHash::from_str(&item.utxo_commitment)?,
        timestamp: item.timestamp.try_into()?,
        bits: item.bits,
        nonce: item.nonce,
        daa_score: item.daa_score,
        blue_work: kaspa_rpc_core::RpcBlueWorkType::from_rpc_hex(&item.blue_work)?,
        blue_score: item.blue_score,
        pruning_point: RpcHash::from_str(&item.pruning_point)?,
    }
});

try_from!(item: &protowire::RpcBlockLevelParents, Vec<RpcHash>, {
    item.parent_hashes.iter().map(|x| RpcHash::from_str(x)).collect::<Result<Vec<_>, _>>()?
});

#[cfg(test)]
mod tests {
    use crate::protowire;
    use kaspa_consensus_core::{block::Block, header::Header};
    use kaspa_rpc_core::{RpcBlock, RpcHash, RpcHeader};

    fn new_unique() -> RpcHash {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let c = COUNTER.fetch_add(1, Ordering::Relaxed);
        RpcHash::from_u64_word(c)
    }

    fn test_parents_by_level_rxr(r: &[Vec<RpcHash>], r2: &[Vec<RpcHash>]) {
        for i in 0..r.len() {
            for j in 0..r[i].len() {
                assert_eq!(r[i][j], r2[i][j]);
            }
        }
    }
    fn test_parents_by_level_rxp(r: &[Vec<RpcHash>], p: &[protowire::RpcBlockLevelParents]) {
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
        let r = Header::new_finalized(
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
        let r = RpcHeader::from(r);
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

    #[test]
    fn test_rpc_block() {
        let h = Header::new_finalized(
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
        let b = Block::from_header(h);
        let r: RpcBlock = (&b).into();
        let p: protowire::RpcBlock = (&r).into();
        let r2: RpcBlock = (&p).try_into().unwrap();
        let b2: Block = r2.clone().try_into().unwrap();
        let r3: RpcBlock = (&b2).into();
        let p2: protowire::RpcBlock = (&r3).into();

        assert_eq!(r.header.parents_by_level, r2.header.parents_by_level);
        assert_eq!(p.header.as_ref().unwrap().parents, p2.header.as_ref().unwrap().parents);
        test_parents_by_level_rxr(&r.header.parents_by_level, &r2.header.parents_by_level);
        test_parents_by_level_rxr(&r.header.parents_by_level, &r3.header.parents_by_level);
        test_parents_by_level_rxr(&b.header.parents_by_level, &r2.header.parents_by_level);
        test_parents_by_level_rxr(&b.header.parents_by_level, &b2.header.parents_by_level);
        test_parents_by_level_rxp(&r.header.parents_by_level, &p.header.as_ref().unwrap().parents);
        test_parents_by_level_rxp(&r.header.parents_by_level, &p2.header.as_ref().unwrap().parents);
        test_parents_by_level_rxp(&r2.header.parents_by_level, &p2.header.as_ref().unwrap().parents);

        assert_eq!(b.hash(), b2.hash());
        assert_eq!(p, p2);
    }
}
