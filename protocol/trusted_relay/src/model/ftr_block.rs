/// Domain models and conversions for Fast Trusted Relay.
///
/// Defines `FtrBlock` (transport-level reassembled block) and conversion
/// functions to consensus-layer `Block` types.
use std::sync::Arc;

use kaspa_consensus_core::{block::Block, header::Header, tx::Transaction};
use kaspa_hashes::Hash;

#[derive(Clone)]
pub struct FtrBlock(pub Vec<u8>);

impl FtrBlock {
    const HeaderHeightIndexOffset: usize = 32;
    const TransactionLengthIndexOffset: usize = 36;

    pub fn new(hash: Hash, header_len: u32, txs_len: u32, header: Vec<u8>, txs: Vec<u8>) -> Self {
        let mut buf: Vec<u8> = Vec::with_capacity(32 + 4 + 4 + header.len() + txs.len());
        buf.extend_from_slice(&hash.as_bytes());
        buf.extend_from_slice(&header_len.to_le_bytes());
        buf.extend_from_slice(&txs_len.to_le_bytes());
        buf.extend_from_slice(&header);
        buf.extend_from_slice(&txs);
        Self(buf)
    }

    #[inline(always)]
    pub fn hash(&self) -> Hash {
        Hash::from_slice(&self.0[..Self::HeaderHeightIndexOffset])
    }

    #[inline(always)]
    pub fn header_len(&self) -> u32 {
        u32::from_le_bytes(
            self.0[Self::HeaderHeightIndexOffset..Self::TransactionLengthIndexOffset]
                .try_into()
                .expect("FtrBlock data too small for header_len"),
        )
    }

    #[inline(always)]
    pub fn txs_len(&self) -> u32 {
        u32::from_le_bytes(
            self.0[Self::TransactionLengthIndexOffset..Self::TransactionLengthIndexOffset + 4]
                .try_into()
                .expect("FtrBlock data too small for txs_len"),
        )
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[inline(always)]
    pub fn as_bytes(self) -> Vec<u8> {
        self.0
    }
}


impl From<FtrBlock> for Block {
    fn from(ftr_block: FtrBlock) -> Self {
        let header_len = ftr_block.header_len() as usize;
        let txs_len = ftr_block.txs_len() as usize;
        let header_start = 32 + 4 + 4;
        let header_end = header_start + header_len;
        let txs_end = header_end + txs_len;

        let header_bytes = &ftr_block.0[header_start..header_end];
        let txs_bytes = &ftr_block.0[header_end..txs_end];

        let header: Arc<Header> = Arc::new(bincode::deserialize(header_bytes).expect("header deserialization failed"));
        let transactions: Arc<Vec<Transaction>> =
            Arc::new(bincode::deserialize(txs_bytes).expect("transactions deserialization failed"));

        Block::from_arcs(header, transactions)
    }
}

impl From<Block> for FtrBlock {

    fn from(block: Block) -> Self {
        let header_bytes = bincode::serialize(&*block.header).expect("header serialization failed");
        let txs_bytes =
            bincode::serialize(&*block.transactions).expect("transactions serialization failed");
        let hash = block.header.hash;
        Self::new(hash, header_bytes.len() as u32, txs_bytes.len() as u32, header_bytes, txs_bytes)
    }
}

impl From<&Block> for FtrBlock {
    fn from(block: &Block) -> Self {
        let header_bytes = bincode::serialize(&*block.header).expect("header serialization failed");
        let txs_bytes =
            bincode::serialize(&*block.transactions).expect("transactions serialization failed");
        let hash = block.header.hash;
        Self::new(hash, header_bytes.len() as u32, txs_bytes.len() as u32, header_bytes, txs_bytes)
    }
}

impl From<Vec<u8>> for FtrBlock {
    fn from(val: Vec<u8>) -> Self {
        FtrBlock(val)
    }
}

impl AsRef<[u8]> for FtrBlock {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ftr_block_creation() {
        let hash = Hash::from([1u8; 32]);
        let data = vec![1, 2, 3, 4];
        let block = FtrBlock::new(hash, 4u32, 0u32, data.clone(), vec![]);
        assert_eq!(block.hash(), hash);
        assert_eq!(block.header_len(), 4);
    }

    #[test]
    fn test_ftr_block_conversion() {
        let hash = Hash::from([1u8; 32]);
        let block = FtrBlock::new(hash, 0u32, 3u32, vec![], vec![1, 2, 3]);
        let result: Vec<u8> = block.as_ref().to_vec();
        let ftr_block = FtrBlock::from(result);
        assert_eq!(ftr_block.hash(), hash);
        assert_eq!(ftr_block.header_len(), 0);
        assert_eq!(ftr_block.txs_len(), 3);
        assert_eq!(&ftr_block.0[40..], &[1, 2, 3]);
    }
}
