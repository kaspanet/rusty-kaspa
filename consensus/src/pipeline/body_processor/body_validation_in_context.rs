use super::BlockBodyProcessor;
use crate::{
    errors::{BlockProcessResult, RuleError},
    model::stores::statuses::StatusesStoreReader,
    processes::{
        transaction_validator::{
            tx_validation_in_header_context::{LockTimeArg, LockTimeType},
            TransactionValidator,
        },
        window::WindowManager,
    },
};
use kaspa_consensus_core::block::Block;
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_hashes::Hash;
use once_cell::unsync::Lazy;
use std::sync::Arc;

impl BlockBodyProcessor {
    pub fn validate_body_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_blue_score_and_subsidy(block)?;
        self.check_block_transactions_in_context(block)
    }

    fn check_block_transactions_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // Use lazy evaluation to avoid unnecessary work, as most of the time we expect the txs not to have lock time.
        let lazy_pmt_res = Lazy::new(|| self.window_manager.calc_past_median_time_for_known_hash(block.hash()));

        for tx in block.transactions.iter() {
            let lock_time_arg = match TransactionValidator::get_lock_time_type(tx) {
                LockTimeType::Finalized => LockTimeArg::Finalized,
                LockTimeType::DaaScore => LockTimeArg::DaaScore(block.header.daa_score),
                // We only evaluate the pmt calculation when actually needed
                LockTimeType::Time => LockTimeArg::MedianTime((*lazy_pmt_res).clone()?),
            };
            if let Err(e) = self.transaction_validator.validate_tx_in_header_context(tx, block.header.daa_score, lock_time_arg) {
                return Err(RuleError::TxInContextFailed(tx.id(), e));
            };
        }
        Ok(())
    }

    fn check_parent_bodies_exist(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let statuses_read_guard = self.statuses_store.read();
        let missing: Vec<Hash> = block
            .header
            .direct_parents()
            .iter()
            .copied()
            .filter(|parent| {
                let status_option = statuses_read_guard.get(*parent).unwrap_option();
                status_option.is_none_or(|s| !s.has_block_body())
            })
            .collect();
        if !missing.is_empty() {
            return Err(RuleError::MissingParents(missing));
        }

        Ok(())
    }

    fn check_coinbase_blue_score_and_subsidy(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        match self.coinbase_manager.deserialize_coinbase_payload(&block.transactions[0].payload) {
            Ok(data) => {
                if data.blue_score != block.header.blue_score {
                    return Err(RuleError::BadCoinbasePayloadBlueScore(data.blue_score, block.header.blue_score));
                }

                let expected_subsidy = self.coinbase_manager.calc_block_subsidy(block.header.daa_score);

                if data.subsidy != expected_subsidy {
                    return Err(RuleError::WrongSubsidy(expected_subsidy, data.subsidy));
                }

                Ok(())
            }
            Err(e) => Err(RuleError::BadCoinbasePayload(e)),
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        config::ConfigBuilder,
        consensus::test_consensus::TestConsensus,
        constants::TX_VERSION,
        errors::RuleError,
        model::stores::ghostdag::GhostdagStoreReader,
        params::DEVNET_PARAMS,
        processes::{transaction_validator::errors::TxRuleError, window::WindowManager},
    };
    use kaspa_consensus_core::{
        api::ConsensusApi,
        merkle::calc_hash_merkle_root,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_core::assert_match;
    use kaspa_hashes::Hash;

    #[tokio::test]
    async fn validate_body_in_context_test() {
        let config = ConfigBuilder::new(DEVNET_PARAMS)
            .skip_proof_of_work()
            .edit_consensus_params(|p| p.deflationary_phase_daa_score = 2)
            .build();
        let consensus = TestConsensus::new(&config);
        let wait_handles = consensus.init();
        let body_processor = consensus.block_body_processor();

        consensus.add_header_only_block_with_parents(1.into(), vec![config.genesis.hash]).await.unwrap();

        {
            let block = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
            // We expect a missing parents error since the parent is header only.
            assert_match!(body_processor.validate_body_in_context(&block.to_immutable()), Err(RuleError::MissingParents(_)));
        }

        let valid_block = consensus.build_block_with_parents_and_transactions(3.into(), vec![config.genesis.hash], vec![]);
        consensus.validate_and_insert_block(valid_block.to_immutable()).virtual_state_task.await.unwrap();
        {
            let mut block = consensus.build_block_with_parents_and_transactions(2.into(), vec![3.into()], vec![]);
            block.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 50000000000);

            // The second time we send an invalid block we expect it to be a known invalid.
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::KnownInvalid)
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(4.into(), vec![3.into()], vec![]);
            block.transactions[0].payload[0..8].copy_from_slice(&(100_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayloadBlueScore(_, _))
            );
        }

        {
            let mut block = consensus.build_block_with_parents_and_transactions(5.into(), vec![3.into()], vec![]);
            block.transactions[0].payload = vec![];
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::BadCoinbasePayload(_))
            );
        }

        let valid_block_child = consensus.build_block_with_parents_and_transactions(6.into(), vec![3.into()], vec![]);
        consensus.validate_and_insert_block(valid_block_child.clone().to_immutable()).virtual_state_task.await.unwrap();
        {
            // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
            let mut block = consensus.build_block_with_parents_and_transactions(7.into(), vec![6.into()], vec![]);
            block.transactions[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());
            assert_match!(consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await, Err(RuleError::WrongSubsidy(expected,_)) if expected == 44000000000);
        }

        {
            // Check that the same daa score as the block's daa score or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 8.into(), tip_daa_score + 1, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 9.into(), tip_daa_score, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 10.into(), tip_daa_score - 1, 0, true).await;

            let valid_block_child_gd = consensus.ghostdag_store().get_data(valid_block_child.header.hash).unwrap();
            let (valid_block_child_gd_pmt, _) = consensus.window_manager().calc_past_median_time(&valid_block_child_gd).unwrap();
            let past_median_time = valid_block_child_gd_pmt + 1;

            // Check that the same past median time as the block's or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 11.into(), past_median_time + 1, 0, false)
                .await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 12.into(), past_median_time, 0, false).await;
            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 13.into(), past_median_time - 1, 0, true)
                .await;

            // We check that if the transaction is marked as finalized it'll pass for any lock time.
            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                14.into(),
                past_median_time + 1,
                u64::MAX,
                true,
            )
            .await;

            check_for_lock_time_and_sequence(&consensus, valid_block_child.header.hash, 15.into(), tip_daa_score + 1, u64::MAX, true)
                .await;
        }

        consensus.shutdown(wait_handles);
    }

    async fn check_for_lock_time_and_sequence(
        consensus: &TestConsensus,
        parent: Hash,
        block_hash: Hash,
        lock_time: u64,
        sequence: u64,
        should_pass: bool,
    ) {
        // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
        let block = consensus.build_block_with_parents_and_transactions(
            block_hash,
            vec![parent],
            vec![Transaction::new(
                TX_VERSION,
                vec![TransactionInput::new(TransactionOutpoint::new(1.into(), 0), vec![], sequence, 0)],
                vec![],
                lock_time,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
            )],
        );

        if should_pass {
            consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.unwrap();
        } else {
            assert_match!(
                consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await,
                Err(RuleError::TxInContextFailed(_, e)) if matches!(e, TxRuleError::NotFinalized(_)));
        }
    }
}
