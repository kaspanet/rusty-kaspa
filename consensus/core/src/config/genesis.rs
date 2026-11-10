use crate::{
    block::Block,
    header::{CompressedParents, Header},
    subnets::SUBNETWORK_ID_COINBASE,
    tx::Transaction,
};
use aero_hashes::{Hash, ZERO_HASH};
use aero_muhash::EMPTY_MUHASH;

/// The constants uniquely representing the genesis block
#[derive(Clone, Debug)]
pub struct GenesisBlock {
    pub hash: Hash,
    pub version: u16,
    pub hash_merkle_root: Hash,
    pub utxo_commitment: Hash,
    pub timestamp: u64,
    pub bits: u32,
    pub nonce: u64,
    pub daa_score: u64,
    pub coinbase_payload: &'static [u8],
}

impl GenesisBlock {
    pub fn build_genesis_transactions(&self) -> Vec<Transaction> {
        vec![Transaction::new(0, Vec::new(), Vec::new(), 0, SUBNETWORK_ID_COINBASE, 0, self.coinbase_payload.to_vec())]
    }
}

impl From<&GenesisBlock> for Header {
    fn from(genesis: &GenesisBlock) -> Self {
        Header::new_finalized(
            genesis.version,
            CompressedParents::default(),
            genesis.hash_merkle_root,
            ZERO_HASH,
            genesis.utxo_commitment,
            genesis.timestamp,
            genesis.bits,
            genesis.nonce,
            genesis.daa_score,
            0.into(),
            0,
            ZERO_HASH,
        )
    }
}

impl From<&GenesisBlock> for Block {
    fn from(genesis: &GenesisBlock) -> Self {
        Block::new(genesis.into(), genesis.build_genesis_transactions())
    }
}

impl From<(&Header, &'static [u8])> for GenesisBlock {
    fn from((header, payload): (&Header, &'static [u8])) -> Self {
        Self {
            hash: header.hash,
            version: header.version,
            hash_merkle_root: header.hash_merkle_root,
            utxo_commitment: header.utxo_commitment,
            timestamp: header.timestamp,
            bits: header.bits,
            nonce: header.nonce,
            daa_score: header.daa_score,
            coinbase_payload: payload,
        }
    }
}

/// The genesis block of the block-DAG which serves as the public transaction ledger for the main network.
pub const GENESIS: GenesisBlock = GenesisBlock {
    // Regenerate with `gen_mainnet_genesis` if any field below changes.
    hash: Hash::from_bytes([
        0x62, 0xaa, 0xae, 0xaa, 0x37, 0x8c, 0xc6, 0xd4, 0xc5, 0xd3, 0xeb, 0xc1, 0x40, 0x89, 0x36, 0x3c, 0xcd, 0xbc, 0x86, 0x10, 0x46,
        0x6a, 0x87, 0x19, 0xfc, 0x79, 0x7f, 0x53, 0x48, 0x14, 0xe8, 0xa9,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0x21, 0xb0, 0x4d, 0xb2, 0xec, 0x3b, 0x62, 0x63, 0xa3, 0x19, 0x81, 0x9b, 0xb2, 0x98, 0x11, 0xe7, 0x31, 0x8b, 0x42, 0x18, 0x12,
        0x8b, 0xc3, 0x91, 0xff, 0x6e, 0xd5, 0xd3, 0xb1, 0x2c, 0x0c, 0xcd,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 1794182400000, // 2026-11-09 00:00:00 UTC
    bits: 537542000,          // 0x200a3d70: ~25 hashes per block (100 H/s at 250ms), see `gen_mainnet_genesis`
    nonce: 0,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x80, 0x96, 0x98, 0x00, 0x00, 0x00, 0x00, 0x00, // Subsidy (10_000_000 sompi, the initial block subsidy)
        0x00, 0x00, // Script version
        0x01,       // Varint
        0x00,       // OP-FALSE
        // "aero-mainnet", then a message. Frozen for launch: it is part of the genesis hash, so changing it forks the chain.
        0x61, 0x65, 0x72, 0x6f, 0x2d, 0x6d, 0x61, 0x69, 0x6e, 0x6e, 0x65, 0x74, // aero-mainnet
        0x52, 0x61, 0x6e, 0x64, 0x6f, 0x6d, 0x58, 0x2c, 0x20, 0x32, 0x35, 0x30, 0x6d, 0x73, 0x20, 0x62, // RandomX, 250ms b
        0x6c, 0x6f, 0x63, 0x6b, 0x73, 0x2c, 0x20, 0x31, 0x30, 0x30, 0x4d, 0x20, 0x63, 0x61, 0x70,       // locks, 100M cap
    ],
};

pub const TESTNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0xf8, 0x96, 0xa3, 0x03, 0x48, 0x73, 0xbe, 0x17, 0x39, 0xfc, 0x43, 0x59, 0x23, 0x68, 0x99, 0xfd, 0x3d, 0x65, 0xd2, 0xbc, 0x94,
        0xf9, 0x78, 0x0d, 0xf0, 0xd0, 0xda, 0x3e, 0xb1, 0xcc, 0x43, 0x70,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0x17, 0x34, 0x14, 0x08, 0xa5, 0x72, 0x45, 0x56, 0x50, 0x4d, 0xf4, 0xd6, 0xcf, 0x51, 0x5c, 0xbf, 0xbb, 0x22, 0x04, 0x30, 0xdc,
        0x45, 0x1c, 0x74, 0x3c, 0x22, 0xd5, 0xe9, 0x11, 0x72, 0x0c, 0x2a,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 0x17c5f62fbb6,
    bits: 0x1e7fffff,
    nonce: 0x14582,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                         // Varint
        0x00,                                                                         // OP-FALSE
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74, // kaspa-testnet
    ],
};

pub const SIMNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0x41, 0x1f, 0x8c, 0xd2, 0x6f, 0x3d, 0x41, 0xae, 0xa3, 0x9e, 0x78, 0x57, 0x39, 0x27, 0xda, 0x24, 0xd2, 0x39, 0x95, 0x70, 0x5b,
        0x57, 0x9f, 0x30, 0x95, 0x9b, 0x91, 0x27, 0xe9, 0x6b, 0x79, 0xe3,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0x19, 0x46, 0xd6, 0x29, 0xf7, 0xe9, 0x22, 0xa7, 0xbc, 0xed, 0x59, 0x19, 0x05, 0x21, 0xc3, 0x77, 0x1f, 0x73, 0xd3, 0x52, 0xdd,
        0xbb, 0xb6, 0x86, 0x56, 0x4a, 0xd7, 0xfd, 0x56, 0x85, 0x7c, 0x1b,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 0x17c5f62fbb6,
    bits: 0x207fffff,
    nonce: 0x2,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                   // Varint
        0x00,                                                                   // OP-FALSE
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x2d, 0x73, 0x69, 0x6d, 0x6e, 0x65, 0x74, // kaspa-simnet
    ],
};

pub const DEVNET_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        // Golang devnet genesis hash
        // 0xb3, 0x13, 0x87, 0x0a, 0x32, 0xc7, 0x04, 0xbd, 0xf1, 0x21, 0x4a, 0x3b, 0x27, 0x0c, 0xc4, 0x75, 0xd9, 0x42, 0xc2, 0x09, 0x2d,
        // 0x37, 0x9b, 0xc8, 0x70, 0x0a, 0xb0, 0x43, 0x31, 0x9e, 0xf8,
        // 0x46,
        // New rust devnet genesis hash updated according to the modified bits field (see below)
        0x4c, 0xb4, 0x8d, 0x0b, 0x20, 0x73, 0xb8, 0x02, 0x36, 0x01, 0x45, 0xa1, 0x5a, 0xd1, 0xab, 0xdc, 0x01, 0xd8, 0x9b, 0x5c, 0x2f,
        0xe4, 0x72, 0x26, 0x30, 0xab, 0x9b, 0x5f, 0xe9, 0xdf, 0xc4, 0xf2,
    ]),
    version: 0,
    hash_merkle_root: Hash::from_bytes([
        0x58, 0xab, 0xf2, 0x03, 0x21, 0xd7, 0x07, 0x16, 0x16, 0x2b, 0x6b, 0xf8, 0xd9, 0xf5, 0x89, 0xca, 0x33, 0xae, 0x6e, 0x32, 0xb3,
        0xb1, 0x9a, 0xbb, 0x7f, 0xa6, 0x5d, 0x11, 0x41, 0xa3, 0xf9, 0x4d,
    ]),
    utxo_commitment: EMPTY_MUHASH,
    timestamp: 0x11e9db49828,
    // bits: 525264379, // Golang devnet genesis bits
    bits: 0x1e21bc1c, // Bits with ~testnet-like difficulty for slow devnet start
    nonce: 0x48e5e,
    daa_score: 0,
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                   // Varint
        0x00,                                                                   // OP-FALSE
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x2d, 0x64, 0x65, 0x76, 0x6e, 0x65, 0x74, // kaspa-devnet
    ],
};

pub const TESTNET13_GENESIS: GenesisBlock = GenesisBlock {
    hash: Hash::from_bytes([
        0xcb, 0xc6, 0x63, 0xfa, 0x43, 0x80, 0xa1, 0x89, 0xb1, 0x6c, 0xcb, 0xfc, 0xeb, 0x94, 0xac, 0xce, 0x90, 0x25, 0xb9, 0xd6, 0x41,
        0xdb, 0x96, 0x85, 0x82, 0xe8, 0x57, 0xb2, 0xf4, 0xc9, 0x5a, 0x69,
    ]),
    hash_merkle_root: Hash::from_bytes([
        0x46, 0x7f, 0x8a, 0xc5, 0x3f, 0xf9, 0x8f, 0xe9, 0x3c, 0x96, 0x2d, 0x25, 0xbe, 0x7a, 0x1f, 0xb1, 0x99, 0xa1, 0x10, 0xc8, 0x30,
        0x39, 0xcf, 0x1b, 0x53, 0xeb, 0x38, 0x3c, 0xb3, 0xbf, 0x05, 0xa3,
    ]),
    bits: 504155340, // see `gen_testnet13_genesis`
    #[rustfmt::skip]
    coinbase_payload: &[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // Blue score
        0x00, 0xE1, 0xF5, 0x05, 0x00, 0x00, 0x00, 0x00, // Subsidy
        0x00, 0x00, // Script version
        0x01,                                                                         // Varint
        0x00,                                                                         // OP-FALSE
        0x6b, 0x61, 0x73, 0x70, 0x61, 0x2d, 0x74, 0x65, 0x73, 0x74, 0x6e, 0x65, 0x74, // kaspa-testnet
        0x54, 0x68, 0x65, 0x72, 0x65, 0x20, 0x61, 0x72,                               // There are 10 types of people in the world
        0x65, 0x20, 0x31, 0x30, 0x20, 0x74, 0x79, 0x70,                               // - those who know binary and those who don't
        0x65, 0x73, 0x20, 0x6f, 0x66, 0x20, 0x70, 0x65,
        0x6f, 0x70, 0x6c, 0x65, 0x20, 0x69, 0x6e, 0x20,
        0x74, 0x68, 0x65, 0x20, 0x77, 0x6f, 0x72, 0x6c,
        0x64, 0x20, 0x2d, 0x20, 0x74, 0x68, 0x6f, 0x73,
        0x65, 0x20, 0x77, 0x68, 0x6f, 0x20, 0x6b, 0x6e,
        0x6f, 0x77, 0x20, 0x62, 0x69, 0x6e, 0x61, 0x72,
        0x79, 0x20, 0x61, 0x6e, 0x64, 0x20, 0x74, 0x68,
        0x6f, 0x73, 0x65, 0x20, 0x77, 0x68, 0x6f, 0x20,
        0x64, 0x6f, 0x6e, 0x27, 0x74,
        0x44, 0x41, 0x47, 0x4b, 0x4e, 0x49, 0x47, 0x48, 0x54,                          // DAGKNIGHT
        13, 1                                                                          // TN13, Launch 1
    ],
    ..TESTNET_GENESIS
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::bps::TenBps;
    use crate::merkle::calc_hash_merkle_root;

    fn all_genesses() -> [GenesisBlock; 5] {
        [GENESIS, TESTNET_GENESIS, SIMNET_GENESIS, DEVNET_GENESIS, TESTNET13_GENESIS]
    }

    #[test]
    fn test_genesis_hashes() {
        all_genesses().into_iter().for_each(|genesis| {
            let block: Block = (&genesis).into();
            assert_hashes_eq(calc_hash_merkle_root(block.transactions.iter()), block.header.hash_merkle_root, "hash_merkle_root");
            assert_hashes_eq(block.hash(), genesis.hash, "genesis hash");
        });
    }

    #[test]
    fn gen_testnet13_genesis() {
        let bps = TenBps::bps();
        let mut genesis = TESTNET_GENESIS;
        let target = aero_math::Uint256::from_compact_target_bits(genesis.bits);
        let scaled_target = target * bps / 100;
        let scaled_bits = scaled_target.compact_target_bits();
        genesis.bits = scaled_bits;
        if genesis.bits != TESTNET13_GENESIS.bits {
            panic!("Testnet 13: new bits: {}\nnew hash: {:#04x?}", scaled_bits, Block::from(&genesis).hash().as_bytes());
        }
    }

    /// Mainnet starts at difficulty tuned for `MAINNET_INITIAL_HASHRATE` hashes per second of RandomX, so that blocks
    /// arrive at the 250ms (4 BPS) target from the first block. Genesis `bits` is used as-is until the difficulty
    /// window fills, after which the DAA takes over and follows the real hashrate in either direction.
    ///
    /// To change the starting hashrate: edit the constant, run this test, and copy the printed `bits` and `hash` into
    /// `GENESIS` (the payload, and so the merkle root, is unaffected).
    #[test]
    fn gen_mainnet_genesis() {
        const MAINNET_INITIAL_HASHRATE: u64 = 100; // H/s: a single low-end CPU
        let hashes_per_block = MAINNET_INITIAL_HASHRATE * crate::config::bps::Bps::<4>::target_time_per_block() / 1000;
        let target = aero_math::Uint256::MAX / hashes_per_block;

        let mut genesis = GENESIS;
        genesis.bits = target.compact_target_bits();
        genesis.hash_merkle_root = calc_hash_merkle_root(Block::from(&genesis).transactions.iter());
        let hash = Block::from(&genesis).hash();
        if genesis.bits != GENESIS.bits || genesis.hash_merkle_root != GENESIS.hash_merkle_root || hash != GENESIS.hash {
            panic!(
                "Mainnet: bits: {} ({:#x}) for ~{} hashes/block\nhash_merkle_root: {:#04x?}\nhash: {:#04x?}",
                genesis.bits,
                genesis.bits,
                hashes_per_block,
                genesis.hash_merkle_root.as_bytes(),
                hash.as_bytes()
            );
        }
    }

    fn assert_hashes_eq(got: Hash, expected: Hash, field: &str) {
        if got != expected {
            // Special hex print to ease changing the genesis hash according to the print if needed
            panic!("Invalid field '{}': Got hash {:#04x?} while expecting {:#04x?}", field, got.as_bytes(), expected.as_bytes());
        }
    }
}
