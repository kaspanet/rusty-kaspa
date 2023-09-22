#[cfg(test)]
mod tests {
    use crate::{
        block_template::builder::BlockTemplateBuilder,
        errors::{MiningManagerError, MiningManagerResult},
        manager::MiningManager,
        mempool::{
            config::{Config, DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE},
            errors::RuleError,
            tx::{Orphan, Priority},
        },
        model::candidate_tx::CandidateTransaction,
        testutils::consensus_mock::ConsensusMock,
        MiningCounters,
    };
    use kaspa_addresses::{Address, Prefix, Version};
    use kaspa_consensus_core::{
        api::ConsensusApi,
        block::TemplateBuildMode,
        coinbase::MinerData,
        constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION},
        errors::tx::{TxResult, TxRuleError},
        mass::transaction_estimated_serialized_size,
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{
            scriptvec, MutableTransaction, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint,
            TransactionOutput, UtxoEntry,
        },
    };
    use kaspa_hashes::Hash;
    use kaspa_txscript::{
        pay_to_address_script, pay_to_script_hash_signature_script,
        test_helpers::{create_transaction, op_true_script},
    };
    use std::sync::Arc;
    use tokio::sync::mpsc::{error::TryRecvError, unbounded_channel};

    const TARGET_TIME_PER_BLOCK: u64 = 1_000;
    const MAX_BLOCK_MASS: u64 = 500_000;

    // test_validate_and_insert_transaction verifies that valid transactions were successfully inserted into the mempool.
    #[test]
    fn test_validate_and_insert_transaction() {
        const TX_COUNT: u32 = 10;
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);
        let transactions_to_insert = (0..TX_COUNT).map(|i| create_transaction_with_utxo_entry(i, 0)).collect::<Vec<_>>();
        for transaction in transactions_to_insert.iter() {
            let result = mining_manager.validate_and_insert_mutable_transaction(
                consensus.as_ref(),
                transaction.clone(),
                Priority::Low,
                Orphan::Allowed,
            );
            assert!(result.is_ok(), "inserting a valid transaction failed");
        }

        // The UtxoEntry was filled manually for those transactions, so the transactions won't be considered orphans.
        // Therefore, all the transactions expected to be contained in the mempool.
        let (transactions_from_pool, _) = mining_manager.get_all_transactions(true, false);
        assert_eq!(
            transactions_to_insert.len(),
            transactions_from_pool.len(),
            "wrong number of transactions in mempool: expected: {}, got: {}",
            transactions_to_insert.len(),
            transactions_from_pool.len()
        );
        transactions_to_insert.iter().for_each(|tx_to_insert| {
            let found_exact_match = transactions_from_pool.contains(tx_to_insert);
            let tx_from_pool = transactions_from_pool.iter().find(|tx_from_pool| tx_from_pool.id() == tx_to_insert.id());
            let found_transaction_id = tx_from_pool.is_some();
            if found_transaction_id && !found_exact_match {
                let tx = tx_from_pool.unwrap();
                assert_eq!(
                    tx_to_insert.calculated_fee.unwrap(),
                    tx.calculated_fee.unwrap(),
                    "wrong fee in transaction {}: expected: {}, got: {}",
                    tx.id(),
                    tx_to_insert.calculated_fee.unwrap(),
                    tx.calculated_fee.unwrap()
                );
                assert_eq!(
                    tx_to_insert.calculated_mass.unwrap(),
                    tx.calculated_mass.unwrap(),
                    "wrong mass in transaction {}: expected: {}, got: {}",
                    tx.id(),
                    tx_to_insert.calculated_mass.unwrap(),
                    tx.calculated_mass.unwrap()
                );
            }
            assert!(found_exact_match, "missing transaction {} in the mempool, no exact match", tx_to_insert.id());
        });

        // The parent's transaction was inserted into the consensus, so we want to verify that
        // the child transaction is not considered an orphan and inserted into the mempool.
        let transaction_not_an_orphan = create_child_and_parent_txs_and_add_parent_to_consensus(&consensus);
        let result = mining_manager.validate_and_insert_transaction(
            consensus.as_ref(),
            transaction_not_an_orphan.clone(),
            Priority::Low,
            Orphan::Allowed,
        );
        assert!(result.is_ok(), "inserting the child transaction {} into the mempool failed", transaction_not_an_orphan.id());
        let (transactions_from_pool, _) = mining_manager.get_all_transactions(true, false);
        assert!(
            contained_by(transaction_not_an_orphan.id(), &transactions_from_pool),
            "missing transaction {} in the mempool",
            transaction_not_an_orphan.id()
        );
    }

    /// test_simulated_error_in_consensus verifies that a predefined result is actually
    /// returned by the consensus mock as expected when the mempool tries to validate and
    /// insert a transaction.
    #[test]
    fn test_simulated_error_in_consensus() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        // Build an invalid transaction with some gas and inform the consensus mock about the result it should return
        // when the mempool will submit this transaction for validation.
        let mut transaction = create_transaction_with_utxo_entry(0, 1);
        Arc::make_mut(&mut transaction.tx).gas = 1000;
        let status = Err(TxRuleError::TxHasGas);
        consensus.set_status(transaction.id(), status.clone());

        // Try validate and insert the transaction into the mempool
        let result = into_status(mining_manager.validate_and_insert_transaction(
            consensus.as_ref(),
            transaction.tx.as_ref().clone(),
            Priority::Low,
            Orphan::Allowed,
        ));

        assert_eq!(
            status, result,
            "Unexpected result when trying to insert an invalid transaction: expected: {status:?}, got: {result:?}",
        );
        let pool_tx = mining_manager.get_transaction(&transaction.id(), true, true);
        assert!(pool_tx.is_none(), "Mempool contains a transaction that should have been rejected");
    }

    /// test_insert_double_transactions_to_mempool verifies that an attempt to insert a transaction
    /// more than once into the mempool will result in raising an appropriate error.
    #[test]
    fn test_insert_double_transactions_to_mempool() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        let transaction = create_transaction_with_utxo_entry(0, 0);

        // submit the transaction to the mempool
        let result = mining_manager.validate_and_insert_mutable_transaction(
            consensus.as_ref(),
            transaction.clone(),
            Priority::Low,
            Orphan::Allowed,
        );
        assert!(result.is_ok(), "mempool should have accepted a valid transaction but did not");

        // submit the same transaction again to the mempool
        let result = mining_manager.validate_and_insert_transaction(
            consensus.as_ref(),
            transaction.tx.as_ref().clone(),
            Priority::Low,
            Orphan::Allowed,
        );
        assert!(result.is_err(), "mempool should refuse a double submit of the same transaction but accepts it");
        if let Err(MiningManagerError::MempoolError(RuleError::RejectDuplicate(transaction_id))) = result {
            assert_eq!(
                transaction.id(),
                transaction_id,
                "the error returned by the mempool should include id {} but provides {}",
                transaction.id(),
                transaction_id
            );
        } else {
            panic!(
                "the nested error returned by the mempool should be variant RuleError::RejectDuplicate but is {:?}",
                result.err().unwrap()
            );
        }
    }

    // test_double_spend_in_mempool verifies that an attempt to insert a transaction double-spending
    // another transaction already in the mempool will result in raising an appropriate error.
    #[test]
    fn test_double_spend_in_mempool() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        let transaction = create_child_and_parent_txs_and_add_parent_to_consensus(&consensus);
        assert!(
            consensus.can_finance_transaction(&MutableTransaction::from_tx(transaction.clone())),
            "the consensus mock should have spendable UTXOs for the newly created transaction {}",
            transaction.id()
        );

        let result =
            mining_manager.validate_and_insert_transaction(consensus.as_ref(), transaction.clone(), Priority::Low, Orphan::Allowed);
        assert!(result.is_ok(), "the mempool should accept a valid transaction when it is able to populate its UTXO entries");

        let mut double_spending_transaction = transaction.clone();
        double_spending_transaction.outputs[0].value -= 1; // do some minor change so that txID is different
        double_spending_transaction.finalize();
        assert_ne!(
            transaction.id(),
            double_spending_transaction.id(),
            "two transactions differing by only one output value should have different ids"
        );
        let result = mining_manager.validate_and_insert_transaction(
            consensus.as_ref(),
            double_spending_transaction.clone(),
            Priority::Low,
            Orphan::Allowed,
        );
        assert!(result.is_err(), "mempool should refuse a double spend transaction but accepts it");
        if let Err(MiningManagerError::MempoolError(RuleError::RejectDoubleSpendInMempool(_, transaction_id))) = result {
            assert_eq!(
                transaction.id(),
                transaction_id,
                "the error returned by the mempool should include id {} but provides {}",
                transaction.id(),
                transaction_id
            );
        } else {
            panic!(
                "the nested error returned by the mempool should be variant RuleError::RejectDoubleSpendInMempool but is {:?}",
                result.err().unwrap()
            );
        }
    }

    // test_handle_new_block_transactions verifies that all the transactions in the block were successfully removed from the mempool.
    #[test]
    fn test_handle_new_block_transactions() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        const TX_COUNT: u32 = 10;
        let transactions_to_insert = (0..TX_COUNT).map(|i| create_transaction_with_utxo_entry(i, 0)).collect::<Vec<_>>();
        for transaction in transactions_to_insert.iter() {
            let result = mining_manager.validate_and_insert_transaction(
                consensus.as_ref(),
                transaction.tx.as_ref().clone(),
                Priority::Low,
                Orphan::Allowed,
            );
            assert!(result.is_ok(), "the insertion of a new valid transaction in the mempool failed");
        }

        const PARTIAL_LEN: usize = 3;
        let (first_part, rest) = transactions_to_insert.split_at(PARTIAL_LEN);

        let block_with_first_part = build_block_transactions(first_part.iter().map(|mtx| mtx.tx.as_ref()));
        let block_with_rest = build_block_transactions(rest.iter().map(|mtx| mtx.tx.as_ref()));

        let result = mining_manager.handle_new_block_transactions(consensus.as_ref(), 2, &block_with_first_part);
        assert!(
            result.is_ok(),
            "the handling by the mempool of the transactions of a block accepted by the consensus should succeed but returned {result:?}"
        );
        for handled_tx_id in first_part.iter().map(|x| x.id()) {
            assert!(
                mining_manager.get_transaction(&handled_tx_id, true, true).is_none(),
                "the transaction {handled_tx_id} should not be in the mempool"
            );
        }
        // There are no chained/double-spends transactions, and hence it is expected that all the other
        // transactions, will still be included in the mempool.
        for handled_tx_id in rest.iter().map(|x| x.id()) {
            assert!(
                mining_manager.get_transaction(&handled_tx_id, true, true).is_some(),
                "the transaction {handled_tx_id} is lacking from the mempool"
            );
        }

        // Handle all the other transactions.
        let result = mining_manager.handle_new_block_transactions(consensus.as_ref(), 3, &block_with_rest);
        assert!(
            result.is_ok(),
            "the handling by the mempool of the transactions of a block accepted by the consensus should succeed but returned {result:?}"            
        );
        for handled_tx_id in rest.iter().map(|x| x.id()) {
            assert!(
                mining_manager.get_transaction(&handled_tx_id, true, true).is_none(),
                "the transaction {handled_tx_id} should no longer be in the mempool"
            );
        }
    }

    #[test]
    // test_double_spend_with_block verifies that any transactions which are now double spends as a result of the block's new transactions
    // will be removed from the mempool.
    fn test_double_spend_with_block() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        let transaction_in_the_mempool = create_transaction_with_utxo_entry(0, 0);
        let result = mining_manager.validate_and_insert_transaction(
            consensus.as_ref(),
            transaction_in_the_mempool.tx.as_ref().clone(),
            Priority::Low,
            Orphan::Allowed,
        );
        assert!(result.is_ok());

        let mut double_spend_transaction_in_the_block = create_transaction_with_utxo_entry(0, 0);
        Arc::make_mut(&mut double_spend_transaction_in_the_block.tx).inputs[0].previous_outpoint =
            transaction_in_the_mempool.tx.inputs[0].previous_outpoint;
        let block_transactions = build_block_transactions(std::iter::once(double_spend_transaction_in_the_block.tx.as_ref()));

        let result = mining_manager.handle_new_block_transactions(consensus.as_ref(), 2, &block_transactions);
        assert!(result.is_ok());

        assert!(
            mining_manager.get_transaction(&transaction_in_the_mempool.id(), true, true).is_none(),
            "the transaction {} shouldn't be in the mempool since at least one output was already spent",
            transaction_in_the_mempool.id()
        );
    }

    // test_orphan_transactions verifies that a transaction could be a part of a new block template only if it's not an orphan.
    #[test]
    fn test_orphan_transactions() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        // Before each parent transaction we add a transaction that funds it and insert the funding transaction in the consensus.
        const TX_PAIRS_COUNT: usize = 5;
        let (parent_txs, child_txs) = create_arrays_of_parent_and_children_transactions(&consensus, TX_PAIRS_COUNT);

        assert_eq!(parent_txs.len(), TX_PAIRS_COUNT);
        assert_eq!(child_txs.len(), TX_PAIRS_COUNT);
        for orphan in child_txs.iter() {
            let result =
                mining_manager.validate_and_insert_transaction(consensus.as_ref(), orphan.clone(), Priority::Low, Orphan::Allowed);
            assert!(result.is_ok(), "the mempool should accept the valid orphan transaction {}", orphan.id());
        }
        let (populated_txs, orphans) = mining_manager.get_all_transactions(true, true);
        assert!(populated_txs.is_empty(), "the mempool should have no populated transaction since only orphans were submitted");
        for orphan in orphans.iter() {
            assert!(
                contained_by(orphan.id(), &child_txs),
                "orphan transaction {} should exist in the child transactions",
                orphan.id()
            );
        }
        for child in child_txs.iter() {
            assert!(contained_by(child.id(), &orphans), "child transaction {} should exist in the orphan pool", child.id());
        }

        // Try to build a block template.
        // It is expected to only contain a coinbase transaction since all children are orphans.
        let miner_data = get_miner_data(Prefix::Testnet);
        let result = mining_manager.get_block_template(consensus.as_ref(), &miner_data);
        assert!(result.is_ok(), "failed at getting a block template");

        let template = result.unwrap();
        for block_tx in template.block.transactions.iter().skip(1) {
            assert!(
                !contained_by(block_tx.id(), &child_txs),
                "transaction {} is an orphan and is found in a built block template",
                block_tx.id()
            );
        }

        // Simulate a block having been added to consensus with all but the first parent transactions.
        const SKIPPED_TXS: usize = 1;
        mining_manager.clear_block_template();
        let added_parent_txs = parent_txs.iter().skip(SKIPPED_TXS).cloned().collect::<Vec<_>>();
        added_parent_txs.iter().for_each(|x| consensus.add_transaction(x.clone(), 1));
        let result =
            mining_manager.handle_new_block_transactions(consensus.as_ref(), 2, &build_block_transactions(added_parent_txs.iter()));
        assert!(result.is_ok(), "mining manager should handle new block transactions successfully but returns {result:?}");
        let unorphaned_txs = result.unwrap();
        let (populated_txs, orphans) = mining_manager.get_all_transactions(true, true);
        assert_eq!(
            unorphaned_txs.len(), child_txs.len() - SKIPPED_TXS,
            "the mempool is expected to have unorphaned all but one child transactions after all but one parent transactions were accepted by the consensus: expected: {}, got: {}",
            unorphaned_txs.len(), child_txs.len() - SKIPPED_TXS
        );
        assert_eq!(
            child_txs.len() - SKIPPED_TXS, populated_txs.len(),
            "the mempool is expected to contain all but one child transactions after all but one parent transactions were accepted by the consensus: expected: {}, got: {}",
            child_txs.len() - SKIPPED_TXS, populated_txs.len()
        );
        for populated in populated_txs.iter() {
            assert!(
                contained_by(populated.id(), &unorphaned_txs),
                "mempool transaction {} should exist in the unorphaned transactions",
                populated.id()
            );
            assert!(
                contained_by(populated.id(), &child_txs),
                "mempool transaction {} should exist in the child transactions",
                populated.id()
            );
        }
        for child in child_txs.iter().skip(SKIPPED_TXS) {
            assert!(
                contained_by(child.id(), &unorphaned_txs),
                "child transaction {} should exist in the unorphaned transactions",
                child.id()
            );
            assert!(contained_by(child.id(), &populated_txs), "child transaction {} should exist in the mempool", child.id());
        }
        assert_eq!(
            SKIPPED_TXS, orphans.len(),
            "the orphan pool is expected to contain one child transaction after all but one parent transactions were accepted by the consensus: expected: {}, got: {}",
            SKIPPED_TXS, orphans.len()
        );
        for orphan in orphans.iter() {
            assert!(
                contained_by(orphan.id(), &child_txs),
                "orphan transaction {} should exist in the child transactions",
                orphan.id()
            );
        }
        for child in child_txs.iter().take(SKIPPED_TXS) {
            assert!(contained_by(child.id(), &orphans), "child transaction {} should exist in the orphan pool", child.id());
        }

        // Build a new block template with all ready transactions, meaning all child transactions but one.
        // Note that the call to get_block_template will actually build a new block template and not use the
        // cached block because clear_block_template was called manually. This call is normally initiated by
        // the flow context OnNewBlockTemplate but wasn't in the context of this unit test.
        let result = mining_manager.get_block_template(consensus.as_ref(), &miner_data);
        assert!(result.is_ok(), "failed at getting a block template");

        let template = result.unwrap();
        assert_eq!(
            populated_txs.len(),
            template.block.transactions.len() - 1,
            "build block template should contain all ready child transactions: expected: {}, got: {}",
            populated_txs.len(),
            template.block.transactions.len() - 1
        );
        for block_tx in template.block.transactions.iter().skip(1) {
            assert!(
                contained_by(block_tx.id(), &child_txs),
                "transaction {} in the built block template does not exist in ready child transactions",
                block_tx.id()
            );
        }
        for child in child_txs.iter().skip(SKIPPED_TXS) {
            assert!(
                contained_by(child.id(), &template.block.transactions),
                "child transaction {} in the mempool was ready but is not found in the built block template",
                child.id()
            )
        }

        // Simulate the built block being added to consensus
        mining_manager.clear_block_template();
        let added_child_txs = child_txs.iter().skip(SKIPPED_TXS).cloned().collect::<Vec<_>>();
        added_child_txs.iter().for_each(|x| consensus.add_transaction(x.clone(), 2));
        let result =
            mining_manager.handle_new_block_transactions(consensus.as_ref(), 4, &build_block_transactions(added_child_txs.iter()));
        assert!(result.is_ok(), "mining manager should handle new block transactions successfully but returns {result:?}");

        let unorphaned_txs = result.unwrap();
        let (populated_txs, orphans) = mining_manager.get_all_transactions(true, true);
        assert_eq!(
            0,
            unorphaned_txs.len(),
            "the unorphaned transaction set should be empty: expected: {}, got: {}",
            0,
            unorphaned_txs.len()
        );
        assert_eq!(0, populated_txs.len(), "the mempool should be empty: expected: {}, got: {}", 0, populated_txs.len());
        assert_eq!(
            1,
            orphans.len(),
            "the orphan pool should contain one remaining child transaction: expected: {}, got: {}",
            1,
            orphans.len()
        );

        // Add the remaining parent transaction into the mempool
        let result =
            mining_manager.validate_and_insert_transaction(consensus.as_ref(), parent_txs[0].clone(), Priority::Low, Orphan::Allowed);
        assert!(result.is_ok(), "the insertion of the remaining parent transaction in the mempool failed");
        let unorphaned_txs = result.unwrap();
        let (populated_txs, orphans) = mining_manager.get_all_transactions(true, true);
        assert_eq!(
            unorphaned_txs.len(), SKIPPED_TXS + 1,
            "the mempool is expected to have unorphaned the remaining child transaction after the matching parent transaction was inserted into the mempool: expected: {}, got: {}",
            SKIPPED_TXS + 1, unorphaned_txs.len()
        );
        assert_eq!(
            SKIPPED_TXS + SKIPPED_TXS,
            populated_txs.len(),
            "the mempool is expected to contain the remaining child/parent transactions pair: expected: {}, got: {}",
            SKIPPED_TXS + SKIPPED_TXS,
            populated_txs.len()
        );
        for parent in parent_txs.iter().take(SKIPPED_TXS) {
            assert!(
                contained_by(parent.id(), &populated_txs),
                "mempool transaction {} should exist in the remaining parent transactions",
                parent.id()
            );
        }
        for child in child_txs.iter().take(SKIPPED_TXS) {
            assert!(
                contained_by(child.id(), &populated_txs),
                "mempool transaction {} should exist in the remaining child transactions",
                child.id()
            );
        }
        assert_eq!(0, orphans.len(), "the orphan pool is expected to be empty: {}, got: {}", 0, orphans.len());
    }

    /// test_high_priority_transactions verifies that inserting a high priority orphan transaction when the orphan pool is full
    /// evicts a low-priority transaction, if available, or fails if the pool is already filled with high priority transactions.
    #[test]
    fn test_high_priority_transactions() {
        struct TestStep {
            name: &'static str,
            priority: Priority,
            should_enter_orphan_pool: bool,
            should_unorphan: bool,
        }

        impl TestStep {
            fn insert_result(&self) -> &'static str {
                match self.should_enter_orphan_pool {
                    false => "rejected by",
                    true => "inserted into",
                }
            }

            fn parent_insert_result(&self) -> &'static str {
                match (self.should_enter_orphan_pool, self.should_unorphan) {
                    (false, _) => "rejected by",
                    (true, false) => "remove from",
                    (true, true) => "inserted into",
                }
            }
        }

        let tests = vec![
            TestStep {
                name: "low-priority transaction into an empty orphan pool",
                priority: Priority::Low,
                should_enter_orphan_pool: true,
                should_unorphan: false,
            },
            TestStep {
                name: "high-priority transaction into a non-full orphan pool",
                priority: Priority::High,
                should_enter_orphan_pool: true,
                should_unorphan: true,
            },
            TestStep {
                name: "high-priority transaction into an orphan pool having some low-priority tx",
                priority: Priority::High,
                should_enter_orphan_pool: true,
                should_unorphan: true,
            },
            TestStep {
                name: "low-priority transaction into an orphan pool filled with high-priority only txs",
                priority: Priority::Low,
                should_enter_orphan_pool: false,
                should_unorphan: false,
            },
            TestStep {
                name: "high-priority transaction into an orphan pool filled with high-priority only txs",
                priority: Priority::Low,
                should_enter_orphan_pool: false,
                should_unorphan: false,
            },
        ];

        let consensus = Arc::new(ConsensusMock::new());
        let mut config = Config::build_default(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS);
        // Limit the orphan pool to 2 transactions
        config.maximum_orphan_transaction_count = 2;
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::with_config(config.clone(), None, counters);

        // Create pairs of transaction parent-and-child pairs according to the test vector
        let (parent_txs, child_txs) = create_arrays_of_parent_and_children_transactions(&consensus, tests.len());

        // Try submit children while rejecting orphans
        for (tx, test) in child_txs.iter().zip(tests.iter()) {
            let result =
                mining_manager.validate_and_insert_transaction(consensus.as_ref(), tx.clone(), test.priority, Orphan::Forbidden);
            assert!(result.is_err(), "mempool should reject an orphan transaction with {:?} when asked to do so", test.priority);
            if let Err(MiningManagerError::MempoolError(RuleError::RejectDisallowedOrphan(transaction_id))) = result {
                assert_eq!(
                    tx.id(),
                    transaction_id,
                    "the error returned by the mempool should include id {} but provides {}",
                    tx.id(),
                    transaction_id
                );
            } else {
                panic!(
                    "the nested error returned by the mempool should be variant RuleError::RejectDisallowedOrphan but is {:?}",
                    result.err().unwrap()
                );
            }
        }

        // Try submit children while accepting orphans
        for (tx, test) in child_txs.iter().zip(tests.iter()) {
            let result =
                mining_manager.validate_and_insert_transaction(consensus.as_ref(), tx.clone(), test.priority, Orphan::Allowed);
            assert_eq!(
                test.should_enter_orphan_pool,
                result.is_ok(),
                "{}: child transaction should be {} the orphan pool",
                test.name,
                test.insert_result()
            );
            if let Ok(unorphaned_txs) = result {
                assert!(unorphaned_txs.is_empty(), "mempool should unorphan no transaction since it only contains orphans");
            } else if let Err(MiningManagerError::MempoolError(RuleError::RejectOrphanPoolIsFull(pool_len, config_len))) = result {
                assert_eq!(
                    (config.maximum_orphan_transaction_count as usize, config.maximum_orphan_transaction_count),
                    (pool_len, config_len),
                    "the error returned by the mempool should include id {:?} but provides {:?}",
                    (config.maximum_orphan_transaction_count as usize, config.maximum_orphan_transaction_count),
                    (pool_len, config_len),
                );
            } else {
                panic!(
                    "the nested error returned by the mempool should be variant RuleError::RejectOrphanPoolIsFull but is {:?}",
                    result.err().unwrap()
                );
            }
        }

        // Submit all the parents
        for (i, (tx, test)) in parent_txs.iter().zip(tests.iter()).enumerate() {
            let result =
                mining_manager.validate_and_insert_transaction(consensus.as_ref(), tx.clone(), test.priority, Orphan::Allowed);
            assert!(result.is_ok(), "mempool should accept a valid transaction with {:?} when asked to do so", test.priority,);
            let unorphaned_txs = result.as_ref().unwrap();
            assert_eq!(
                test.should_unorphan,
                unorphaned_txs.len() > 1,
                "{}: child transaction should have been {} the orphan pool",
                test.name,
                test.parent_insert_result()
            );
            if unorphaned_txs.len() > 1 {
                assert_eq!(unorphaned_txs[1].id(), child_txs[i].id(), "the unorphaned transaction should match the inserted parent");
            }
        }
    }

    /// test_revalidate_high_priority_transactions verifies that a transaction spending an output of a transaction initially
    /// accepted by the consensus is later removed from the mempool when the funding transaction gets invalidated in consensus
    /// by a reorg.
    #[test]
    fn test_revalidate_high_priority_transactions() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        // Create two valid transactions that double-spend each other (child_tx_1, child_tx_2)
        let (parent_tx, child_tx_1) = create_parent_and_children_transactions(&consensus, vec![3000 * SOMPI_PER_KASPA]);
        consensus.add_transaction(parent_tx, 0);

        let mut child_tx_2 = child_tx_1.clone();
        child_tx_2.outputs[0].value -= 1; // decrement value to change id
        child_tx_2.finalize();

        // Simulate: Mine 1 block with confirming child_tx_1 and 2 blocks confirming child_tx_2, so that
        // child_tx_2 is accepted
        consensus.add_transaction(child_tx_2.clone(), 3);

        // Add to mempool a transaction that spends child_tx_2 (as high priority)
        let spending_tx = create_transaction(&child_tx_2, 1_000);
        let result =
            mining_manager.validate_and_insert_transaction(consensus.as_ref(), spending_tx.clone(), Priority::High, Orphan::Allowed);
        assert!(result.is_ok(), "the insertion in the mempool of the spending transaction failed");

        // Revalidate, to make sure spending_tx is still valid
        let (tx, mut rx) = unbounded_channel();
        mining_manager.revalidate_high_priority_transactions(consensus.as_ref(), tx);
        let result = rx.blocking_recv();
        assert!(result.is_some(), "the revalidation of high-priority transactions must yield one message");
        assert_eq!(
            Err(TryRecvError::Disconnected),
            rx.try_recv(),
            "the revalidation of high-priority transactions must yield exactly one message"
        );
        let valid_txs = result.unwrap();
        assert_eq!(1, valid_txs.len(), "the revalidated transaction count is wrong: expected: {}, got: {}", 1, valid_txs.len());
        assert_eq!(spending_tx.id(), valid_txs[0], "the revalidated transaction is not the right one");

        // Simulate: Mine 2 more blocks on top of tip1, to re-org out child_tx_1, thus making spending_tx invalid
        consensus.add_transaction(child_tx_1, 1);
        consensus.set_status(spending_tx.id(), Err(TxRuleError::MissingTxOutpoints));

        // Make sure spending_tx is still in mempool
        assert!(
            mining_manager.get_transaction(&spending_tx.id(), true, false).is_some(),
            "the spending transaction is no longer in the mempool"
        );

        // Revalidate again, this time valid_txs should be empty
        let (tx, mut rx) = unbounded_channel();
        mining_manager.revalidate_high_priority_transactions(consensus.as_ref(), tx);
        assert_eq!(
            Err(TryRecvError::Disconnected),
            rx.try_recv(),
            "the revalidation of high-priority transactions must yield no message"
        );

        // And the mempool should be empty too
        let (populated_txs, orphan_txs) = mining_manager.get_all_transactions(true, true);
        assert!(populated_txs.is_empty(), "mempool should be empty");
        assert!(orphan_txs.is_empty(), "orphan pool should be empty");
    }

    // test_modify_block_template verifies that modifying a block template changes coinbase data correctly.
    #[test]
    fn test_modify_block_template() {
        let consensus = Arc::new(ConsensusMock::new());
        let counters = Arc::new(MiningCounters::default());
        let mining_manager = MiningManager::new(TARGET_TIME_PER_BLOCK, false, MAX_BLOCK_MASS, None, counters);

        // Before each parent transaction we add a transaction that funds it and insert the funding transaction in the consensus.
        const TX_PAIRS_COUNT: usize = 12;
        let (parent_txs, child_txs) = create_arrays_of_parent_and_children_transactions(&consensus, TX_PAIRS_COUNT);

        for (parent_tx, child_tx) in parent_txs.iter().zip(child_txs.iter()) {
            let result =
                mining_manager.validate_and_insert_transaction(consensus.as_ref(), parent_tx.clone(), Priority::Low, Orphan::Allowed);
            assert!(result.is_ok(), "the mempool should accept the valid parent transaction {}", parent_tx.id());
            let result =
                mining_manager.validate_and_insert_transaction(consensus.as_ref(), child_tx.clone(), Priority::Low, Orphan::Allowed);
            assert!(result.is_ok(), "the mempool should accept the valid child transaction {}", parent_tx.id());
        }

        // Collect all parent transactions for the next block template.
        // They are ready since they have no parents in the mempool.
        let transactions = mining_manager.block_candidate_transactions();
        assert_eq!(
            TX_PAIRS_COUNT,
            transactions.len(),
            "the mempool should provide all parent transactions as candidates for the next block template"
        );
        parent_txs.iter().for_each(|x| {
            assert!(
                transactions.iter().any(|tx| tx.tx.id() == x.id()),
                "the parent transaction {} should be candidate for the next block template",
                x.id()
            );
        });

        // Test modify block template
        sweep_compare_modified_template_to_built(consensus.as_ref(), Prefix::Testnet, &mining_manager, transactions);

        // TODO: extend the test according to the golang scenario
    }

    fn sweep_compare_modified_template_to_built(
        consensus: &dyn ConsensusApi,
        address_prefix: Prefix,
        mining_manager: &MiningManager,
        transactions: Vec<CandidateTransaction>,
    ) {
        for _ in 0..4 {
            // Run a few times to get more randomness
            compare_modified_template_to_built(
                consensus,
                address_prefix,
                mining_manager,
                transactions.clone(),
                OpType::Usual,
                OpType::Usual,
            );
            compare_modified_template_to_built(
                consensus,
                address_prefix,
                mining_manager,
                transactions.clone(),
                OpType::Edcsa,
                OpType::Edcsa,
            );
        }
        compare_modified_template_to_built(
            consensus,
            address_prefix,
            mining_manager,
            transactions.clone(),
            OpType::True,
            OpType::Usual,
        );
        compare_modified_template_to_built(
            consensus,
            address_prefix,
            mining_manager,
            transactions.clone(),
            OpType::Usual,
            OpType::True,
        );
        compare_modified_template_to_built(
            consensus,
            address_prefix,
            mining_manager,
            transactions.clone(),
            OpType::Edcsa,
            OpType::Usual,
        );
        compare_modified_template_to_built(
            consensus,
            address_prefix,
            mining_manager,
            transactions.clone(),
            OpType::Usual,
            OpType::Edcsa,
        );
        compare_modified_template_to_built(
            consensus,
            address_prefix,
            mining_manager,
            transactions.clone(),
            OpType::Empty,
            OpType::Usual,
        );
        compare_modified_template_to_built(consensus, address_prefix, mining_manager, transactions, OpType::Usual, OpType::Empty);
    }

    fn compare_modified_template_to_built(
        consensus: &dyn ConsensusApi,
        address_prefix: Prefix,
        mining_manager: &MiningManager,
        transactions: Vec<CandidateTransaction>,
        first_op: OpType,
        second_op: OpType,
    ) {
        let miner_data_1 = generate_new_coinbase(address_prefix, first_op);
        let miner_data_2 = generate_new_coinbase(address_prefix, second_op);

        // Build a fresh template for coinbase2 as a reference
        let builder = mining_manager.block_template_builder();
        let result = builder.build_block_template(consensus, &miner_data_2, transactions, TemplateBuildMode::Standard);
        assert!(result.is_ok(), "build block template failed for miner data 2");
        let expected_template = result.unwrap();

        // Modify to miner_data_1
        let result = BlockTemplateBuilder::modify_block_template(consensus, &miner_data_1, &expected_template);
        assert!(result.is_ok(), "modify block template failed for miner data 1");
        let mut modified_template = result.unwrap();
        // Make sure timestamps are equal before comparing the hash
        if modified_template.block.header.timestamp != expected_template.block.header.timestamp {
            modified_template.block.header.timestamp = expected_template.block.header.timestamp;
            modified_template.block.header.finalize();
        }

        // Compare hashes
        let expected_block = expected_template.clone().block.to_immutable();
        let modified_block = modified_template.clone().block.to_immutable();
        assert_ne!(
            expected_template.block.header.hash, modified_template.block.header.hash,
            "built and modified block templates should have different hashes"
        );
        assert_ne!(expected_block.hash(), modified_block.hash(), "built and modified blocks should have different hashes");

        // And modify back to miner_data_2
        let result = BlockTemplateBuilder::modify_block_template(consensus, &miner_data_2, &modified_template);
        assert!(result.is_ok(), "modify block template failed for miner data 2");
        let mut modified_template_2 = result.unwrap();
        // Make sure timestamps are equal before comparing the hash
        if modified_template_2.block.header.timestamp != expected_template.block.header.timestamp {
            modified_template_2.block.header.timestamp = expected_template.block.header.timestamp;
            modified_template_2.block.header.finalize();
        }

        // Compare hashes
        let modified_block = modified_template_2.clone().block.to_immutable();
        assert_eq!(
            expected_template.block.header.hash, modified_template_2.block.header.hash,
            "built and modified block templates should have same hashes"
        );
        assert_eq!(
            expected_block.hash(),
            modified_block.hash(),
            "built and modified block templates should have same hashes \n\n{expected_block:?}\n\n{modified_block:?}\n\n"
        );
    }

    #[derive(Clone, Debug)]
    enum OpType {
        Usual,
        Edcsa,
        True,
        Empty,
    }

    fn generate_new_coinbase(address_prefix: Prefix, op: OpType) -> MinerData {
        match op {
            OpType::Usual => get_miner_data(address_prefix), // TODO: use lib_kaspa_wallet.CreateKeyPair, util.NewAddressPublicKeyECDSA equivalents
            OpType::Edcsa => get_miner_data(address_prefix), // TODO: use lib_kaspa_wallet.CreateKeyPair, util.NewAddressPublicKey equivalents
            OpType::True => {
                let (script, _) = op_true_script();
                MinerData::new(script, vec![])
            }
            OpType::Empty => MinerData::new(ScriptPublicKey::new(0, scriptvec![]), vec![]),
        }
    }

    fn create_transaction_with_utxo_entry(i: u32, block_daa_score: u64) -> MutableTransaction {
        let previous_outpoint = TransactionOutpoint::new(Hash::default(), i);
        let (script_public_key, redeem_script) = op_true_script();
        let signature_script = pay_to_script_hash_signature_script(redeem_script, vec![]).expect("the redeem script is canonical");

        let input = TransactionInput::new(previous_outpoint, signature_script, MAX_TX_IN_SEQUENCE_NUM, 1);
        let entry = UtxoEntry::new(SOMPI_PER_KASPA, script_public_key.clone(), block_daa_score, true);
        let output = TransactionOutput::new(SOMPI_PER_KASPA - DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE, script_public_key);
        let transaction = Transaction::new(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);

        let mut mutable_tx = MutableTransaction::from_tx(transaction);
        mutable_tx.calculated_fee = Some(DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE);
        // Please note: this is the ConsensusMock version of the calculated_mass which differs from Consensus
        mutable_tx.calculated_mass = Some(transaction_estimated_serialized_size(&mutable_tx.tx));
        mutable_tx.entries[0] = Some(entry);

        mutable_tx
    }

    fn create_arrays_of_parent_and_children_transactions(
        consensus: &Arc<ConsensusMock>,
        count: usize,
    ) -> (Vec<Transaction>, Vec<Transaction>) {
        // Make the funding amounts always different so that funding txs have different ids
        (0..count)
            .map(|i| {
                create_parent_and_children_transactions(consensus, vec![500 * SOMPI_PER_KASPA, 3_000 * SOMPI_PER_KASPA + i as u64])
            })
            .unzip()
    }

    fn create_parent_and_children_transactions(
        consensus: &Arc<ConsensusMock>,
        funding_amounts: Vec<u64>,
    ) -> (Transaction, Transaction) {
        let funding_tx = create_transaction_without_input(funding_amounts);
        let parent_tx = create_transaction(&funding_tx, DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE);
        let child_tx = create_transaction(&parent_tx, DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE);
        consensus.add_transaction(funding_tx, 1);

        (parent_tx, child_tx)
    }

    fn create_child_and_parent_txs_and_add_parent_to_consensus(consensus: &Arc<ConsensusMock>) -> Transaction {
        let parent_tx = create_transaction_without_input(vec![500 * SOMPI_PER_KASPA]);
        let child_tx = create_transaction(&parent_tx, 1000);
        consensus.add_transaction(parent_tx, 1);
        child_tx
    }

    fn create_transaction_without_input(output_values: Vec<u64>) -> Transaction {
        let (script_public_key, _) = op_true_script();
        let outputs = output_values.iter().map(|value| TransactionOutput::new(*value, script_public_key.clone())).collect();
        Transaction::new(TX_VERSION, vec![], outputs, 0, SUBNETWORK_ID_NATIVE, 0, vec![])
    }

    fn contained_by<T: AsRef<Transaction>>(transaction_id: TransactionId, transactions: &[T]) -> bool {
        transactions.iter().any(|x| x.as_ref().id() == transaction_id)
    }

    fn into_status<T>(result: MiningManagerResult<T>) -> TxResult<()> {
        match result {
            Ok(_) => Ok(()),
            Err(MiningManagerError::MempoolError(RuleError::RejectTxRule(err))) => Err(err),
            _ => Ok(()),
        }
    }

    fn get_dummy_coinbase_tx() -> Transaction {
        Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
    }

    fn build_block_transactions<'a>(transactions: impl Iterator<Item = &'a Transaction>) -> Vec<Transaction> {
        let mut block_transactions = vec![get_dummy_coinbase_tx()];
        block_transactions.extend(transactions.cloned());
        block_transactions
    }

    fn get_miner_data(prefix: Prefix) -> MinerData {
        let secp = secp256k1::Secp256k1::new();
        let mut rng = rand::thread_rng();
        let (_sk, pk) = secp.generate_keypair(&mut rng);
        let address = Address::new(prefix, Version::PubKeyECDSA, &pk.serialize());
        let script = pay_to_address_script(&address);
        MinerData::new(script, vec![])
    }
}
