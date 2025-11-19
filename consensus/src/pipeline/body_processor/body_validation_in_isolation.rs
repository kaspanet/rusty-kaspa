use std::{collections::HashSet, sync::Arc};

use super::BlockBodyProcessor;
use crate::errors::{BlockProcessResult, RuleError};
use kaspa_consensus_core::{
    block::Block,
    mass::{ContextualMasses, Mass, NonContextualMasses},
    merkle::calc_hash_merkle_root,
    tx::TransactionOutpoint,
};

impl BlockBodyProcessor {
    pub fn validate_body_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<Mass> {
        Self::check_has_transactions(block)?;
        Self::check_hash_merkle_root(block)?;
        Self::check_only_one_coinbase(block)?;
        self.check_transactions_in_isolation(block)?;
        let mass = self.check_block_mass(block)?;
        self.check_duplicate_transactions(block)?;
        self.check_block_double_spends(block)?;
        self.check_no_chained_transactions(block)?;
        Ok(mass)
    }

    fn check_has_transactions(block: &Block) -> BlockProcessResult<()> {
        // We expect the outer flow to not queue blocks with no transactions for body validation,
        // but we still check it in case the outer flow changes.
        if block.transactions.is_empty() {
            return Err(RuleError::NoTransactions);
        }
        Ok(())
    }

    fn check_hash_merkle_root(block: &Block) -> BlockProcessResult<()> {
        let calculated = calc_hash_merkle_root(block.transactions.iter());
        if calculated != block.header.hash_merkle_root {
            return Err(RuleError::BadMerkleRoot(block.header.hash_merkle_root, calculated));
        }
        Ok(())
    }

    fn check_only_one_coinbase(block: &Block) -> BlockProcessResult<()> {
        if !block.transactions[0].is_coinbase() {
            return Err(RuleError::FirstTxNotCoinbase);
        }

        if let Some(i) = block.transactions[1..].iter().position(|tx| tx.is_coinbase()) {
            return Err(RuleError::MultipleCoinbases(i));
        }

        Ok(())
    }

    fn check_transactions_in_isolation(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        for tx in block.transactions.iter() {
            if let Err(e) = self.transaction_validator.validate_tx_in_isolation(tx) {
                return Err(RuleError::TxInIsolationValidationFailed(tx.id(), e));
            }
        }
        Ok(())
    }

    fn check_block_mass(self: &Arc<Self>, block: &Block) -> BlockProcessResult<Mass> {
        let mut total_compute_mass: u64 = 0;
        let mut total_transient_mass: u64 = 0;
        let mut total_storage_mass: u64 = 0;
        for tx in block.transactions.iter() {
            // Calculate the non-contextual masses
            let NonContextualMasses { compute_mass, transient_mass } = self.mass_calculator.calc_non_contextual_masses(tx);

            // Read the storage mass commitment. This value cannot be computed here w/o UTXO context
            // so we use the commitment. Later on, when the transaction is verified in context, we use
            // the context to calculate the expected storage mass and verify it matches this commitment
            let storage_mass_commitment = tx.mass();

            // Sum over the various masses separately
            total_compute_mass = total_compute_mass.saturating_add(compute_mass);
            total_transient_mass = total_transient_mass.saturating_add(transient_mass);
            total_storage_mass = total_storage_mass.saturating_add(storage_mass_commitment);

            // Verify all limits
            if total_compute_mass > self.max_block_mass {
                return Err(RuleError::ExceedsComputeMassLimit(total_compute_mass, self.max_block_mass));
            }
            if total_transient_mass > self.max_block_mass {
                return Err(RuleError::ExceedsTransientMassLimit(total_transient_mass, self.max_block_mass));
            }
            if total_storage_mass > self.max_block_mass {
                return Err(RuleError::ExceedsStorageMassLimit(total_storage_mass, self.max_block_mass));
            }
        }
        Ok((NonContextualMasses::new(total_compute_mass, total_transient_mass), ContextualMasses::new(total_storage_mass)))
    }

    fn check_block_double_spends(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut existing = HashSet::new();
        for input in block.transactions.iter().flat_map(|tx| &tx.inputs) {
            if !existing.insert(input.previous_outpoint) {
                return Err(RuleError::DoubleSpendInSameBlock(input.previous_outpoint));
            }
        }
        Ok(())
    }

    fn check_no_chained_transactions(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut block_created_outpoints = HashSet::new();
        for tx in block.transactions.iter() {
            for index in 0..tx.outputs.len() {
                block_created_outpoints.insert(TransactionOutpoint { transaction_id: tx.id(), index: index as u32 });
            }
        }

        for input in block.transactions.iter().flat_map(|tx| &tx.inputs) {
            if block_created_outpoints.contains(&input.previous_outpoint) {
                return Err(RuleError::ChainedTransaction(input.previous_outpoint));
            }
        }
        Ok(())
    }

    fn check_duplicate_transactions(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let mut ids = HashSet::new();
        for tx in block.transactions.iter() {
            if !ids.insert(tx.id()) {
                return Err(RuleError::DuplicateTransactions(tx.id()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::{Config, ConfigBuilder},
        consensus::test_consensus::TestConsensus,
        errors::RuleError,
        params::MAINNET_PARAMS,
    };
    use kaspa_consensus_core::{
        api::{BlockValidationFutures, ConsensusApi},
        block::MutableBlock,
        header::Header,
        merkle::calc_hash_merkle_root,
        subnets::{SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE},
        tx::{scriptvec, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    use kaspa_core::assert_match;
    use kaspa_hashes::Hash;

    #[test]
    fn validate_body_in_isolation_test() {
        let consensus = TestConsensus::new(&Config::new(MAINNET_PARAMS));
        let wait_handles = consensus.init();

        let body_processor = consensus.block_body_processor();
        let example_block = MutableBlock::new(
            Header::new_finalized(
                0,
                vec![vec![
                    Hash::from_slice(&[
                        0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3, 0x46, 0x11,
                        0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                    ]),
                    Hash::from_slice(&[
                        0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87, 0x2a, 0x04,
                        0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                    ]),
                ]],
                Hash::from_slice(&[
                    0x46, 0xec, 0xf4, 0x5b, 0xe3, 0xba, 0xca, 0x34, 0x9d, 0xfe, 0x8a, 0x78, 0xde, 0xaf, 0x05, 0x3b, 0x0a, 0xa6, 0xd5,
                    0x38, 0x97, 0x4d, 0xa5, 0x0f, 0xd6, 0xef, 0xb4, 0xd2, 0x66, 0xbc, 0x8d, 0x21,
                ]),
                Default::default(),
                Default::default(),
                0x17305aa654a,
                0x207fffff,
                1,
                0,
                0.into(),
                9,
                Default::default(),
            ),
            vec![
                Transaction::new(
                    0,
                    vec![],
                    vec![TransactionOutput {
                        value: 0x12a05f200,
                        script_public_key: ScriptPublicKey::new(
                            0,
                            scriptvec!(
                                0xa9, 0x14, 0xda, 0x17, 0x45, 0xe9, 0xb5, 0x49, 0xbd, 0x0b, 0xfa, 0x1a, 0x56, 0x99, 0x71, 0xc7, 0x7e,
                                0xba, 0x30, 0xcd, 0x5a, 0x4b, 0x87
                            ),
                        ),
                    }],
                    0,
                    SUBNETWORK_ID_COINBASE,
                    0,
                    vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                ),
                Transaction::new(
                    0,
                    vec![
                        TransactionInput {
                            previous_outpoint: TransactionOutpoint {
                                transaction_id: TransactionId::from_slice(&[
                                    0x16, 0x5e, 0x38, 0xe8, 0xb3, 0x91, 0x45, 0x95, 0xd9, 0xc6, 0x41, 0xf3, 0xb8, 0xee, 0xc2, 0xf3,
                                    0x46, 0x11, 0x89, 0x6b, 0x82, 0x1a, 0x68, 0x3b, 0x7a, 0x4e, 0xde, 0xfe, 0x2c, 0x00, 0x00, 0x00,
                                ]),
                                index: 0xffffffff,
                            },
                            signature_script: vec![],
                            sequence: u64::MAX,
                            sig_op_count: 0,
                        },
                        TransactionInput {
                            previous_outpoint: TransactionOutpoint {
                                transaction_id: TransactionId::from_slice(&[
                                    0x4b, 0xb0, 0x75, 0x35, 0xdf, 0xd5, 0x8e, 0x0b, 0x3c, 0xd6, 0x4f, 0xd7, 0x15, 0x52, 0x80, 0x87,
                                    0x2a, 0x04, 0x71, 0xbc, 0xf8, 0x30, 0x95, 0x52, 0x6a, 0xce, 0x0e, 0x38, 0xc6, 0x00, 0x00, 0x00,
                                ]),
                                index: 0xffffffff,
                            },
                            signature_script: vec![],
                            sequence: u64::MAX,
                            sig_op_count: 0,
                        },
                    ],
                    vec![],
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    vec![],
                ),
                Transaction::new(
                    0,
                    vec![TransactionInput {
                        previous_outpoint: TransactionOutpoint {
                            transaction_id: TransactionId::from_slice(&[
                                0x03, 0x2e, 0x38, 0xe9, 0xc0, 0xa8, 0x4c, 0x60, 0x46, 0xd6, 0x87, 0xd1, 0x05, 0x56, 0xdc, 0xac, 0xc4,
                                0x1d, 0x27, 0x5e, 0xc5, 0x5f, 0xc0, 0x07, 0x79, 0xac, 0x88, 0xfd, 0xf3, 0x57, 0xa1, 0x87,
                            ]),
                            index: 0,
                        },
                        signature_script: vec![
                            0x49, // OP_DATA_73
                            0x30, 0x46, 0x02, 0x21, 0x00, 0xc3, 0x52, 0xd3, 0xdd, 0x99, 0x3a, 0x98, 0x1b, 0xeb, 0xa4, 0xa6, 0x3a,
                            0xd1, 0x5c, 0x20, 0x92, 0x75, 0xca, 0x94, 0x70, 0xab, 0xfc, 0xd5, 0x7d, 0xa9, 0x3b, 0x58, 0xe4, 0xeb,
                            0x5d, 0xce, 0x82, 0x02, 0x21, 0x00, 0x84, 0x07, 0x92, 0xbc, 0x1f, 0x45, 0x60, 0x62, 0x81, 0x9f, 0x15,
                            0xd3, 0x3e, 0xe7, 0x05, 0x5c, 0xf7, 0xb5, 0xee, 0x1a, 0xf1, 0xeb, 0xcc, 0x60, 0x28, 0xd9, 0xcd, 0xb1,
                            0xc3, 0xaf, 0x77, 0x48, 0x01, // 73-byte signature
                            0x41, // OP_DATA_65
                            0x04, 0xf4, 0x6d, 0xb5, 0xe9, 0xd6, 0x1a, 0x9d, 0xc2, 0x7b, 0x8d, 0x64, 0xad, 0x23, 0xe7, 0x38, 0x3a,
                            0x4e, 0x6c, 0xa1, 0x64, 0x59, 0x3c, 0x25, 0x27, 0xc0, 0x38, 0xc0, 0x85, 0x7e, 0xb6, 0x7e, 0xe8, 0xe8,
                            0x25, 0xdc, 0xa6, 0x50, 0x46, 0xb8, 0x2c, 0x93, 0x31, 0x58, 0x6c, 0x82, 0xe0, 0xfd, 0x1f, 0x63, 0x3f,
                            0x25, 0xf8, 0x7c, 0x16, 0x1b, 0xc6, 0xf8, 0xa6, 0x30, 0x12, 0x1d, 0xf2, 0xb3, 0xd3, // 65-byte pubkey
                        ],
                        sequence: u64::MAX,
                        sig_op_count: 0,
                    }],
                    vec![
                        TransactionOutput {
                            value: 0x2123e300,
                            script_public_key: ScriptPublicKey::new(
                                0,
                                scriptvec!(
                                    0x76, // OP_DUP
                                    0xa9, // OP_HASH160
                                    0x14, // OP_DATA_20
                                    0xc3, 0x98, 0xef, 0xa9, 0xc3, 0x92, 0xba, 0x60, 0x13, 0xc5, 0xe0, 0x4e, 0xe7, 0x29, 0x75, 0x5e,
                                    0xf7, 0xf5, 0x8b, 0x32, 0x88, // OP_EQUALVERIFY
                                    0xac  // OP_CHECKSIG
                                ),
                            ),
                        },
                        TransactionOutput {
                            value: 0x108e20f00,
                            script_public_key: ScriptPublicKey::new(
                                0,
                                scriptvec!(
                                    0x76, // OP_DUP
                                    0xa9, // OP_HASH160
                                    0x14, // OP_DATA_20
                                    0x94, 0x8c, 0x76, 0x5a, 0x69, 0x14, 0xd4, 0x3f, 0x2a, 0x7a, 0xc1, 0x77, 0xda, 0x2c, 0x2f, 0x6b,
                                    0x52, 0xde, 0x3d, 0x7c, 0x88, // OP_EQUALVERIFY
                                    0xac  // OP_CHECKSIG
                                ),
                            ),
                        },
                    ],
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    vec![],
                ),
                Transaction::new(
                    0,
                    vec![TransactionInput {
                        previous_outpoint: TransactionOutpoint {
                            transaction_id: TransactionId::from_slice(&[
                                0xc3, 0x3e, 0xbf, 0xf2, 0xa7, 0x09, 0xf1, 0x3d, 0x9f, 0x9a, 0x75, 0x69, 0xab, 0x16, 0xa3, 0x27, 0x86,
                                0xaf, 0x7d, 0x7e, 0x2d, 0xe0, 0x92, 0x65, 0xe4, 0x1c, 0x61, 0xd0, 0x78, 0x29, 0x4e, 0xcf,
                            ]),
                            index: 1,
                        },
                        signature_script: vec![
                            0x47, // OP_DATA_71
                            0x30, 0x44, 0x02, 0x20, 0x03, 0x2d, 0x30, 0xdf, 0x5e, 0xe6, 0xf5, 0x7f, 0xa4, 0x6c, 0xdd, 0xb5, 0xeb,
                            0x8d, 0x0d, 0x9f, 0xe8, 0xde, 0x6b, 0x34, 0x2d, 0x27, 0x94, 0x2a, 0xe9, 0x0a, 0x32, 0x31, 0xe0, 0xba,
                            0x33, 0x3e, 0x02, 0x20, 0x3d, 0xee, 0xe8, 0x06, 0x0f, 0xdc, 0x70, 0x23, 0x0a, 0x7f, 0x5b, 0x4a, 0xd7,
                            0xd7, 0xbc, 0x3e, 0x62, 0x8c, 0xbe, 0x21, 0x9a, 0x88, 0x6b, 0x84, 0x26, 0x9e, 0xae, 0xb8, 0x1e, 0x26,
                            0xb4, 0xfe, 0x01, 0x41, // OP_DATA_65
                            0x04, 0xae, 0x31, 0xc3, 0x1b, 0xf9, 0x12, 0x78, 0xd9, 0x9b, 0x83, 0x77, 0xa3, 0x5b, 0xbc, 0xe5, 0xb2,
                            0x7d, 0x9f, 0xff, 0x15, 0x45, 0x68, 0x39, 0xe9, 0x19, 0x45, 0x3f, 0xc7, 0xb3, 0xf7, 0x21, 0xf0, 0xba,
                            0x40, 0x3f, 0xf9, 0x6c, 0x9d, 0xee, 0xb6, 0x80, 0xe5, 0xfd, 0x34, 0x1c, 0x0f, 0xc3, 0xa7, 0xb9, 0x0d,
                            0xa4, 0x63, 0x1e, 0xe3, 0x95, 0x60, 0x63, 0x9d, 0xb4, 0x62, 0xe9, 0xcb, 0x85, 0x0f, // 65-byte pubkey
                        ],
                        sequence: u64::MAX,
                        sig_op_count: 0,
                    }],
                    vec![
                        TransactionOutput {
                            value: 0xf4240,
                            script_public_key: ScriptPublicKey::new(
                                0,
                                scriptvec!(
                                    0x76, // OP_DUP
                                    0xa9, // OP_HASH160
                                    0x14, // OP_DATA_20
                                    0xb0, 0xdc, 0xbf, 0x97, 0xea, 0xbf, 0x44, 0x04, 0xe3, 0x1d, 0x95, 0x24, 0x77, 0xce, 0x82, 0x2d,
                                    0xad, 0xbe, 0x7e, 0x10, 0x88, // OP_EQUALVERIFY
                                    0xac  // OP_CHECKSIG
                                ),
                            ),
                        },
                        TransactionOutput {
                            value: 0x11d260c0,
                            script_public_key: ScriptPublicKey::new(
                                0,
                                scriptvec!(
                                    0x76, // OP_DUP
                                    0xa9, // OP_HASH160
                                    0x14, // OP_DATA_20
                                    0x6b, 0x12, 0x81, 0xee, 0xc2, 0x5a, 0xb4, 0xe1, 0xe0, 0x79, 0x3f, 0xf4, 0xe0, 0x8a, 0xb1, 0xab,
                                    0xb3, 0x40, 0x9c, 0xd9, 0x88, // OP_EQUALVERIFY
                                    0xac  // OP_CHECKSIG
                                ),
                            ),
                        },
                    ],
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    vec![],
                ),
                Transaction::new(
                    0,
                    vec![TransactionInput {
                        previous_outpoint: TransactionOutpoint {
                            transaction_id: TransactionId::from_slice(&[
                                0x0b, 0x60, 0x72, 0xb3, 0x86, 0xd4, 0xa7, 0x73, 0x23, 0x52, 0x37, 0xf6, 0x4c, 0x11, 0x26, 0xac, 0x3b,
                                0x24, 0x0c, 0x84, 0xb9, 0x17, 0xa3, 0x90, 0x9b, 0xa1, 0xc4, 0x3d, 0xed, 0x5f, 0x51, 0xf4,
                            ]),
                            index: 0,
                        },
                        signature_script: vec![
                            0x49, // OP_DATA_73
                            0x30, 0x46, 0x02, 0x21, 0x00, 0xbb, 0x1a, 0xd2, 0x6d, 0xf9, 0x30, 0xa5, 0x1c, 0xce, 0x11, 0x0c, 0xf4,
                            0x4f, 0x7a, 0x48, 0xc3, 0xc5, 0x61, 0xfd, 0x97, 0x75, 0x00, 0xb1, 0xae, 0x5d, 0x6b, 0x6f, 0xd1, 0x3d,
                            0x0b, 0x3f, 0x4a, 0x02, 0x21, 0x00, 0xc5, 0xb4, 0x29, 0x51, 0xac, 0xed, 0xff, 0x14, 0xab, 0xba, 0x27,
                            0x36, 0xfd, 0x57, 0x4b, 0xdb, 0x46, 0x5f, 0x3e, 0x6f, 0x8d, 0xa1, 0x2e, 0x2c, 0x53, 0x03, 0x95, 0x4a,
                            0xca, 0x7f, 0x78, 0xf3, 0x01, // 73-byte signature
                            0x41, // OP_DATA_65
                            0x04, 0xa7, 0x13, 0x5b, 0xfe, 0x82, 0x4c, 0x97, 0xec, 0xc0, 0x1e, 0xc7, 0xd7, 0xe3, 0x36, 0x18, 0x5c,
                            0x81, 0xe2, 0xaa, 0x2c, 0x41, 0xab, 0x17, 0x54, 0x07, 0xc0, 0x94, 0x84, 0xce, 0x96, 0x94, 0xb4, 0x49,
                            0x53, 0xfc, 0xb7, 0x51, 0x20, 0x65, 0x64, 0xa9, 0xc2, 0x4d, 0xd0, 0x94, 0xd4, 0x2f, 0xdb, 0xfd, 0xd5,
                            0xaa, 0xd3, 0xe0, 0x63, 0xce, 0x6a, 0xf4, 0xcf, 0xaa, 0xea, 0x4e, 0xa1, 0x4f, 0xbb, // 65-byte pubkey
                        ],
                        sequence: u64::MAX,
                        sig_op_count: 0,
                    }],
                    vec![TransactionOutput {
                        value: 0xf4240,
                        script_public_key: ScriptPublicKey::new(
                            0,
                            scriptvec!(
                                0x76, // OP_DUP
                                0xa9, // OP_HASH160
                                0x14, // OP_DATA_20
                                0x39, 0xaa, 0x3d, 0x56, 0x9e, 0x06, 0xa1, 0xd7, 0x92, 0x6d, 0xc4, 0xbe, 0x11, 0x93, 0xc9, 0x9b, 0xf2,
                                0xeb, 0x9e, 0xe0, 0x88, // OP_EQUALVERIFY
                                0xac  // OP_CHECKSIG
                            ),
                        ),
                    }],
                    0,
                    SUBNETWORK_ID_NATIVE,
                    0,
                    vec![],
                ),
            ],
        );

        body_processor.validate_body_in_isolation(&example_block.clone().to_immutable()).unwrap();

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].version += 1;
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::BadMerkleRoot(_, _)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].inputs[0].sig_op_count = 255;
        txs[1].inputs[1].sig_op_count = 255;
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::ExceedsComputeMassLimit(_, _)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs.push(txs[1].clone());
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::DuplicateTransactions(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].subnetwork_id = SUBNETWORK_ID_COINBASE;
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::MultipleCoinbases(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[2].inputs[0].previous_outpoint = txs[1].inputs[0].previous_outpoint;
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::DoubleSpendInSameBlock(_)));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[0].subnetwork_id = SUBNETWORK_ID_NATIVE;
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::FirstTxNotCoinbase));

        let mut block = example_block.clone();
        let txs = &mut block.transactions;
        txs[1].inputs = vec![];
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(
            body_processor.validate_body_in_isolation(&block.to_immutable()),
            Err(RuleError::TxInIsolationValidationFailed(_, _))
        );

        let mut block = example_block;
        let txs = &mut block.transactions;
        txs[3].inputs[0].previous_outpoint = TransactionOutpoint { transaction_id: txs[2].id(), index: 0 };
        block.header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        assert_match!(body_processor.validate_body_in_isolation(&block.to_immutable()), Err(RuleError::ChainedTransaction(_)));

        consensus.shutdown(wait_handles);
    }

    #[tokio::test]
    async fn merkle_root_missing_parents_known_invalid_test() {
        let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();

        let mut block = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        block.transactions[0].version += 1;

        let BlockValidationFutures { block_task, virtual_state_task } =
            consensus.validate_and_insert_block(block.clone().to_immutable());

        assert_match!(block_task.await, Err(RuleError::BadMerkleRoot(_, _)));
        // Assert that both tasks return the same error
        assert_match!(virtual_state_task.await, Err(RuleError::BadMerkleRoot(_, _)));

        // BadMerkleRoot shouldn't mark the block as known invalid
        assert_match!(
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
            Err(RuleError::BadMerkleRoot(_, _))
        );

        let mut block = consensus.build_block_with_parents_and_transactions(1.into(), vec![config.genesis.hash], vec![]);
        block.header.parents_by_level[0][0] = 0.into();

        assert_match!(
            consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await,
            Err(RuleError::MissingParents(_))
        );

        // MissingParents shouldn't mark the block as known invalid
        assert_match!(
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
            Err(RuleError::MissingParents(_))
        );

        consensus.shutdown(wait_handles);
    }
}
