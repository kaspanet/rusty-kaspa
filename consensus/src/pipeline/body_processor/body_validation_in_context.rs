use super::BlockBodyProcessor;
use crate::{
    errors::{BlockProcessResult, RuleError},
    model::stores::{errors::StoreResultExtensions, ghostdag::GhostdagStoreReader, statuses::StatusesStoreReader},
};
use consensus_core::block::Block;
use hashes::Hash;
use std::sync::Arc;

impl BlockBodyProcessor {
    pub fn validate_body_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        self.check_parent_bodies_exist(block)?;
        self.check_coinbase_subsidy(block)?;
        self.check_block_transactions_in_context(block)?;
        self.check_block_is_not_pruned(block)
    }

    fn check_block_is_not_pruned(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // TODO: In kaspad code it checks that the block is not in the past of the current tips.
        // We should decide what's the best indication that a block was pruned.
        Ok(())
    }

    fn check_block_transactions_in_context(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let (pmt, _) = self
            .past_median_time_manager
            .calc_past_median_time(
                self.ghostdag_store
                    .get_data(block.hash())
                    .unwrap(),
            );
        for tx in block.transactions.iter() {
            if let Err(e) = self
                .transaction_validator
                .utxo_free_tx_validation(tx, block.header.daa_score, pmt)
            {
                return Err(RuleError::TxInContextFailed(tx.id(), e));
            }
        }

        Ok(())
    }

    fn check_parent_bodies_exist(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        // TODO: Skip this check for blocks in PP anticone that comes as part of the pruning proof.

        if block.header.direct_parents().len() == 1 && block.header.direct_parents()[0] == self.genesis_hash {
            return Ok(());
        }

        let statuses_read_guard = self.statuses_store.read();
        let missing: Vec<Hash> = block
            .header
            .direct_parents()
            .iter()
            .cloned()
            .filter(|parent| {
                let status_option = statuses_read_guard.get(*parent).unwrap_option();
                status_option.is_none() || !status_option.unwrap().has_block_body()
            })
            .collect();
        if !missing.is_empty() {
            return Err(RuleError::MissingParents(missing));
        }

        Ok(())
    }

    fn check_coinbase_subsidy(self: &Arc<Self>, block: &Block) -> BlockProcessResult<()> {
        let coinbase_subsidy = self
        .coinbase_manager
        .validate_coinbase_payload_in_isolation_and_extract_coinbase_data(&block.transactions[0])
        .unwrap() // It's ok to unwrap since it was already validated on check_coinbase_in_isolation
        .subsidy;
        let expected_subsidy = self
            .coinbase_manager
            .calc_block_subsidy(block.header.daa_score);
        if coinbase_subsidy != expected_subsidy {
            return Err(RuleError::WrongSubsidy(expected_subsidy, coinbase_subsidy));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        consensus::test_consensus::TestConsensus, constants::TX_VERSION, errors::RuleError,
        model::stores::ghostdag::GhostdagStoreReader, params::MAINNET_PARAMS,
        processes::transaction_validator::errors::TxRuleError,
    };
    use consensus_core::{
        merkle::calc_hash_merkle_root,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };
    use hashes::Hash;
    use std::sync::Arc;

    #[tokio::test]
    async fn validate_body_in_context_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.deflationary_phase_daa_score = 2;
        let consensus = TestConsensus::create_from_temp_db(&params);
        let wait_handles = consensus.init();

        let body_processor = consensus.block_body_processor();

        consensus
            .add_block_with_parents(1.into(), vec![params.genesis_hash])
            .await
            .unwrap();

        {
            let block = consensus.build_block_with_parents_and_transactions(2.into(), vec![1.into()], vec![]);
            // We expect a missing parents error since the parent is header only.
            assert!(matches!(body_processor.validate_body_in_context(&block), Err(RuleError::MissingParents(_))));
        }

        let valid_block =
            consensus.build_block_with_parents_and_transactions(3.into(), vec![params.genesis_hash], vec![]);
        consensus
            .validate_and_insert_block(Arc::new(valid_block))
            .await
            .unwrap();
        {
            let mut block = consensus.build_block_with_parents_and_transactions(2.into(), vec![3.into()], vec![]);
            Arc::make_mut(&mut block.transactions)[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());

            let block = Arc::new(block);
            assert!(
                matches!(consensus.validate_and_insert_block(block.clone()).await,Err(RuleError::WrongSubsidy(expected,_)) if expected == 50000000000)
            );

            // The second time we send an invalid block we expect it to be a known invalid.
            assert!(matches!(consensus.validate_and_insert_block(block).await, Err(RuleError::KnownInvalid)));
        }

        let valid_block_child =
            Arc::new(consensus.build_block_with_parents_and_transactions(4.into(), vec![3.into()], vec![]));
        consensus
            .validate_and_insert_block(valid_block_child.clone())
            .await
            .unwrap();
        {
            // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
            let mut block = consensus.build_block_with_parents_and_transactions(5.into(), vec![4.into()], vec![]);
            Arc::make_mut(&mut block.transactions)[0].payload[8..16].copy_from_slice(&(5_u64).to_le_bytes());
            block.header.hash_merkle_root = calc_hash_merkle_root(block.transactions.iter());
            assert!(
                matches!(consensus.validate_and_insert_block(Arc::new(block)).await,Err(RuleError::WrongSubsidy(expected,_)) if expected == 44000000000)
            );
        }

        {
            // Check that the same daa score as the block's daa score or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                6.into(),
                tip_daa_score + 1,
                0,
                false,
            )
            .await;

            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                7.into(),
                tip_daa_score,
                0,
                false,
            )
            .await;

            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                8.into(),
                tip_daa_score - 1,
                0,
                true,
            )
            .await;

            let valid_block_child_gd = consensus
                .ghostdag_store()
                .get_data(valid_block_child.header.hash)
                .unwrap();
            let (valid_block_child_gd_pmt, _) = consensus
                .past_median_time_manager()
                .calc_past_median_time(valid_block_child_gd);
            let past_median_time = valid_block_child_gd_pmt + 1;

            // Check that the same past median time as the block's or higher fails, but lower passes.
            let tip_daa_score = valid_block_child.header.daa_score + 1;
            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                9.into(),
                past_median_time + 1,
                0,
                false,
            )
            .await;

            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                10.into(),
                past_median_time,
                0,
                false,
            )
            .await;

            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                11.into(),
                past_median_time - 1,
                0,
                true,
            )
            .await;

            // We check that if the transaction is marked as finalized it'll pass for any lock time.
            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                12.into(),
                past_median_time + 1,
                u64::MAX,
                true,
            )
            .await;

            check_for_lock_time_and_sequence(
                &consensus,
                valid_block_child.header.hash,
                13.into(),
                tip_daa_score + 1,
                u64::MAX,
                true,
            )
            .await;
        }

        consensus.shutdown(wait_handles);
    }

    async fn check_for_lock_time_and_sequence(
        consensus: &TestConsensus, parent: Hash, block_hash: Hash, lock_time: u64, sequence: u64, should_pass: bool,
    ) {
        // The block DAA score is 2, so the subsidy should be calculated according to the deflationary stage.
        let block = consensus.build_block_with_parents_and_transactions(
            block_hash,
            vec![4.into()],
            vec![Transaction::new(
                TX_VERSION,
                vec![Arc::new(TransactionInput::new(TransactionOutpoint::new(1.into(), 0), vec![], sequence, 0, None))],
                vec![],
                lock_time,
                SUBNETWORK_ID_NATIVE,
                0,
                vec![],
                0,
            )],
        );

        if should_pass {
            consensus
                .validate_and_insert_block(Arc::new(block))
                .await
                .unwrap();
        } else {
            assert!(matches!(
                consensus
                    .validate_and_insert_block(Arc::new(block))
                    .await,
                Err(RuleError::TxInContextFailed(_, e)) if matches!(e, TxRuleError::NotFinalized(_))
            ));
        }
    }
}
