use std::{collections::HashMap, thread::sleep, time::Duration};

use crate::{
    consensus::test_consensus::TestConsensus,
    model::stores::{
        acceptance_data::AcceptanceDataStoreReader, pruning::PruningStoreReader, selected_chain::SelectedChainStoreReader,
    },
    pipeline::virtual_processor::tests_util::TestContext,
    processes::reachability::tests::gen::generate_complex_dag,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    blockstatus::BlockStatus,
    config::{
        params::{ForkActivation, MAINNET_PARAMS},
        ConfigBuilder,
    },
};

#[tokio::test]
async fn test_receipts_in_chain() {
    const PERIODS: usize = 5;
    const FINALITY_DEPTH: usize = 20;
    const BPS: f64 = 10.0;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.prior_max_block_parents = 10; // Is probably enough to avoid errors
            p.crescendo.max_block_parents = 10;
            p.prior_mergeset_size_limit = 30;
            p.crescendo.mergeset_size_limit = 30;
            p.prior_ghostdag_k = 4;
            p.crescendo.ghostdag_k = 4;
            p.prior_finality_depth = FINALITY_DEPTH as u64;
            p.crescendo.finality_depth = FINALITY_DEPTH as u64;
            p.prior_pruning_depth = (FINALITY_DEPTH * 3 - 5) as u64;
            p.crescendo.pruning_depth = (FINALITY_DEPTH * 3 - 5) as u64;
            p.crescendo_activation = ForkActivation::always();
            p.prior_target_time_per_block = (1000.0 / BPS) as u64;
            p.crescendo.target_time_per_block = (1000.0 / BPS) as u64;
        })
        .build();
    let mut expected_posterities = vec![];
    let mut receipts = vec![];

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut tip = genesis_hash; // Compulsory initialization

    // Mine enough blocks to reach first pruning point
    expected_posterities.push(genesis_hash);
    for _ in 0..3 {
        for _ in 0..FINALITY_DEPTH {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
    }
    // Check genesis behavior
    let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(genesis_hash);
    let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(genesis_hash);
    assert_eq!(pre_posterity, expected_posterities[0]);
    assert_eq!(post_posterity.unwrap(), expected_posterities[1]);
    assert!(ctx.tx_receipts_manager().verify_post_posterity_block(genesis_hash, expected_posterities[1]));

    let mut it = ctx.consensus.services.reachability_service.forward_chain_iterator(genesis_hash, tip, true).skip(1);
    for i in 0..PERIODS - 3 {
        for _ in 0..FINALITY_DEPTH - 1 {
            // This loop:
            // A) creates a new block and adds it at the tip
            // B) validates the posterity qualitiesof the block 3*FINALITY_DEPTH blocks in its past

            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
            let block = it.next().unwrap();
            let acc_tx = ctx.consensus.acceptance_data_store.get(block).unwrap()[0].accepted_transactions[0].transaction_id;
            receipts.push((ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap(), acc_tx));
            let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(block);
            let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(block);
            assert_eq!(pre_posterity, expected_posterities[i]);
            assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
            assert!(ctx.tx_receipts_manager().verify_post_posterity_block(block, expected_posterities[i + 1]));
        }
        // Insert and update the next posterity block
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
        // Verify posterity qualities of a 3*FINALITY_DEPTH blocks in the past posterity block
        let past_posterity_block = it.next().unwrap();
        let acc_tx = ctx.consensus.acceptance_data_store.get(past_posterity_block).unwrap()[0].accepted_transactions[0].transaction_id;
        receipts.push((ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap(), acc_tx));
        let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(past_posterity_block);
        let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(past_posterity_block);
        assert_eq!(pre_posterity, expected_posterities[i]);
        assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
        assert!(ctx.tx_receipts_manager().verify_post_posterity_block(past_posterity_block, expected_posterities[i + 2]));
        // Update the iterator
        it = ctx.consensus.services.reachability_service.forward_chain_iterator(past_posterity_block, tip, true).skip(1);
    }

    for _ in 0..FINALITY_DEPTH / 2 {
        // Insert an extra few blocks, not enough for a new posterity
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    // Check remaining blocks, which were yet to be pruned
    for i in PERIODS - 3..PERIODS + 1 {
        for block in it.by_ref().take(FINALITY_DEPTH - 1) {
            let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(block);
            let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(block);

            assert_eq!(pre_posterity, expected_posterities[i]);
            if i == PERIODS {
                assert!(post_posterity.is_err());
            } else {
                assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
                assert!(ctx.tx_receipts_manager().verify_post_posterity_block(block, expected_posterities[i + 1]));
            }
        }
        if i == PERIODS {
            break;
        }
        let past_posterity_block = it.next().unwrap();
        let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(past_posterity_block);
        let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(past_posterity_block);
        assert_eq!(pre_posterity, expected_posterities[i]);
        // Edge case logic
        if i == PERIODS - 1 {
            assert!(post_posterity.is_err());
        } else {
            assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
        }
    }

    for block in it {
        // Check final blocks
        let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(block);
        let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(block);
        assert_eq!(pre_posterity, expected_posterities[PERIODS]);
        assert!(post_posterity.is_err());
    }
    for (rec, tx_id) in receipts {
        assert!(ctx.consensus.verify_tx_receipt(&rec));
        // sanity check
        assert_eq!(rec.tracked_tx_id, tx_id);
    }
}
#[tokio::test]
async fn test_receipts_in_random() {
    // The test generates a random dag and builds blocks from it, with some modifications to the dag to make it more realistic
    // whenever a new posterity block is reached by some margin, a batch of receipts is attempted to be generated for old blocks.
    // These receipts are then verified at the end of the test.
    // Rarely the test fails because the real posterity of a block will change after a receipt for it has been pulled:
    // the security margin should be enough for real data but test data behaves unexpectedly and thus this error persists despite attempts to mitigate it
    // This error appears to decrease with higher BPS

    // Perhaps this test needs just be replaced by a simulation on real data.
    const FINALITY_DEPTH: usize = 10;
    const DAG_SIZE: u64 = 500;
    const BPS: f64 = 10.0;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.prior_max_block_parents = 10; // Is probably enough to avoid errors
            p.crescendo.max_block_parents = 10;
            p.prior_mergeset_size_limit = 30;
            p.crescendo.mergeset_size_limit = 30;
            p.prior_ghostdag_k = 4;
            p.crescendo.ghostdag_k = 4;
            p.prior_finality_depth = FINALITY_DEPTH as u64;
            p.crescendo.finality_depth = FINALITY_DEPTH as u64;
            p.prior_pruning_depth = (FINALITY_DEPTH * 3 - 5) as u64;
            p.crescendo.pruning_depth = (FINALITY_DEPTH * 3 - 5) as u64;
            p.crescendo_activation = ForkActivation::always();
        })
        .build();
    let mut receipts1 = std::collections::HashMap::<_, _>::new();
    let mut receipts2 = std::collections::HashMap::<_, _>::new();
    let mut receipts3 = std::collections::HashMap::<_, _>::new();

    let ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut posterity_list = vec![genesis_hash];

    let dag = generate_complex_dag(2.0, BPS, DAG_SIZE);
    eprintln!("{dag:?}");
    let mut next_posterity_score = FINALITY_DEPTH as u64;
    let mut mapper = HashMap::new();
    mapper.insert(dag.0, genesis_hash);
    //  A loop over the simulated dag is created, converting it step by step to a blockDag
    //  Mapper is the hash map coupling the abstract nodes on the dag object to the block hashes.
    //  Some of the vertices and nodes of the original node are changed so the blockdag behaves more realistically
    for (ind, parents_ind) in dag.1.into_iter() {
        let mut parents = vec![];
        for par in parents_ind.clone().iter() {
            if let Some(&par_mapped) = mapper.get(par) {
                // Avoid pointing on pending blocks, as pointing on them propogates the pending status forwards making the test meaningless
                if [BlockStatus::StatusUTXOValid, BlockStatus::StatusDisqualifiedFromChain]
                    .contains(&ctx.consensus.block_status(par_mapped))
                {
                    parents.push(par_mapped);
                }
            }
        }
        // Make sure not all parents have been removed, if so ignore this node and do not attempt to convert it to a block.
        if parents.is_empty() {
            {
                continue;
            }
        }

        // Add the new block to the blockdag, and update it on mapper
        mapper.insert(ind, ind.into());

        ctx.add_utxo_valid_block_with_parents(mapper[&ind], parents, vec![]).await;
        // Periodically check if a new posterity point has been reached
        // If so, attempt to create and store receipts and POPs for a batch of blocks past the current pruning point.
        if ctx.consensus.is_posterity_reached(next_posterity_score) {
            sleep(Duration::from_millis(300));

            while ctx.consensus.pruning_point_store.read().retention_checkpoint().unwrap()
                != ctx.consensus.pruning_point_store.read().retention_period_root().unwrap()
            {
                sleep(Duration::from_millis(100));
            } // Delay to prevent pruning races
            next_posterity_score += FINALITY_DEPTH as u64;
            posterity_list.push(ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(ctx.consensus.get_sink()));
            if posterity_list.len() >= 3 {
                for old_block in ctx
                    .consensus
                    .services
                    .reachability_service
                    .forward_chain_iterator(ctx.consensus.pruning_point(), posterity_list[posterity_list.len() - 2], true)
                    .skip(1)
                // Skip pruning_point
                {
                    let blk_header = ctx.consensus.get_header(old_block).unwrap();
                    if old_block != genesis_hash && ctx.consensus.selected_chain_store.read().get_by_hash(old_block).is_ok() {
                        // 1) genesis is an annoying edge case as it has no accepted txs
                        // 2) since the posterity list is merely an apporximation of reality, it is still
                        // required to explicitely check if the blocks are on the selceted chain */
                        let acc_tx =
                            ctx.consensus.acceptance_data_store.get(old_block).unwrap()[0].accepted_transactions[0].transaction_id;
                        receipts1.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, Some(blk_header.hash), None).unwrap());
                        receipts2
                            .insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, None, Some(blk_header.timestamp)).unwrap());

                        receipts3.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap());
                    }
                }
            }
        }
    }
    for blk in ctx.consensus.services.reachability_service.default_backward_chain_iterator(ctx.consensus.get_sink()) {
        eprintln!("chain blk: {:?}", blk);
    }

    for point in posterity_list.clone() {
        eprintln!("candidate posterity hash:{:?}", point);
    }

    for point in ctx.consensus.pruning_point_headers().into_iter() {
        eprintln!("posterity hash:{:?}\n bscore: {:?}", point.hash, point.blue_score);
    }
    eprintln!("receipts:{}", receipts1.len());

    assert!(receipts1.len() >= DAG_SIZE as usize / (4.5 * BPS) as usize); // sanity check

    for rec in receipts1.values() {
        assert!(ctx.consensus.verify_tx_receipt(rec));
    }
    for rec in receipts2.values() {
        assert!(ctx.consensus.verify_tx_receipt(rec));
    }
    for rec in receipts3.values() {
        assert!(ctx.consensus.verify_tx_receipt(rec));
    }
}
