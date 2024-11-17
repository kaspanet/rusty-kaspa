use std::collections::HashMap;

use crate::{
    consensus::test_consensus::TestConsensus,
    model::{
        services::reachability::ReachabilityService,
        stores::{
            acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader,
        },
    },
    pipeline::virtual_processor::tests_util::TestContext,
    processes::reachability::tests::gen::generate_complex_dag,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    blockstatus::BlockStatus,
    config::{params::MAINNET_PARAMS, ConfigBuilder},
};

#[tokio::test]
async fn test_receipts_in_chain() {
    const PERIODS: usize = 5;
    const FINALITY_DEPTH: usize = 20;
    const BPS: f64 = 20.0;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.finality_depth = FINALITY_DEPTH as u64;
            p.target_time_per_block = (1000.0 / BPS) as u64;
            p.pruning_depth = (FINALITY_DEPTH * 3) as u64;
        })
        .build();
    let mut expected_posterities = vec![];
    let mut receipts = vec![];
    let mut pops = vec![];

    let mut pochms_list = vec![];

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut tip = genesis_hash; //compulsory initialization

    // mine enough blocks to reach first pruning point
    expected_posterities.push(genesis_hash);
    for _ in 0..3 {
        for _ in 0..FINALITY_DEPTH {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
    }
    //check genesis behavior
    let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(genesis_hash);
    let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(genesis_hash);
    assert_eq!(pre_posterity, expected_posterities[0]);
    assert_eq!(post_posterity.unwrap(), expected_posterities[1]);
    assert!(ctx.tx_receipts_manager().verify_post_posterity_block(genesis_hash, expected_posterities[1]));

    let mut it = ctx.consensus.services.reachability_service.forward_chain_iterator(genesis_hash, tip, true).skip(1);
    for i in 0..PERIODS - 3 {
        for _ in 0..FINALITY_DEPTH - 1 {
            /*This loop:
            A) creates a new block and adds it at the tip
            B) validates the posterity qualitiesof the block 3*FINALITY_DEPTH blocks in its past*/

            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
            let block = it.next().unwrap();
            let block_header = ctx.consensus.headers_store.get_header(block).unwrap();
            pochms_list.push((ctx.consensus.generate_pochm(block).unwrap(), block));
            let acc_tx = ctx.consensus.acceptance_data_store.get(block).unwrap()[0].accepted_transactions[0].transaction_id;
            receipts
                .push((ctx.consensus.services.tx_receipts_manager.generate_tx_receipt(block_header.clone(), acc_tx).unwrap(), acc_tx)); //add later: test if works via timestamp
            let pub_tx = ctx.consensus.block_transactions_store.get(block).unwrap()[0].id();
            pops.push((ctx.consensus.services.tx_receipts_manager.generate_proof_of_pub(block_header, pub_tx).unwrap(), pub_tx));
            let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(block);
            let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(block);
            assert_eq!(pre_posterity, expected_posterities[i]);
            assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
            assert!(ctx.tx_receipts_manager().verify_post_posterity_block(block, expected_posterities[i + 1]));
        }
        //insert and update the next posterity block
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
        //verify posterity qualities of a 3*FINALITY_DEPTH blocks in the past posterity block
        let past_posterity_block = it.next().unwrap();
        let past_posterity_header = ctx.consensus.headers_store.get_header(past_posterity_block).unwrap();
        pochms_list.push((ctx.consensus.generate_pochm(past_posterity_block).unwrap(), past_posterity_block));

        let acc_tx = ctx.consensus.acceptance_data_store.get(past_posterity_block).unwrap()[0].accepted_transactions[0].transaction_id;
        receipts.push((
            ctx.consensus.services.tx_receipts_manager.generate_tx_receipt(past_posterity_header.clone(), acc_tx).unwrap(),
            acc_tx,
        )); //add later: test if works via timestamp
        let pub_tx = ctx.consensus.block_transactions_store.get(past_posterity_block).unwrap()[0].id();
        pops.push((ctx.consensus.services.tx_receipts_manager.generate_proof_of_pub(past_posterity_header, pub_tx).unwrap(), pub_tx));
        let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(past_posterity_block);
        let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(past_posterity_block);
        assert_eq!(pre_posterity, expected_posterities[i]);
        assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
        assert!(ctx.tx_receipts_manager().verify_post_posterity_block(past_posterity_block, expected_posterities[i + 2]));
        //update the iterator
        it = ctx.consensus.services.reachability_service.forward_chain_iterator(past_posterity_block, tip, true).skip(1);
    }

    for _ in 0..FINALITY_DEPTH / 2 {
        //insert an extra few blocks, not enough for a new posterity
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    //check remaining blocks, which were yet to be pruned
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
        //edge case logic
        if i == PERIODS - 1 {
            assert!(post_posterity.is_err());
        } else {
            assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
        }
    }

    for block in it {
        //check final blocks
        let pre_posterity = ctx.tx_receipts_manager().get_pre_posterity_block_by_hash(block);
        let post_posterity = ctx.tx_receipts_manager().get_post_posterity_block(block);
        assert_eq!(pre_posterity, expected_posterities[PERIODS]);
        assert!(post_posterity.is_err());
    }
    for (pochm, blk) in pochms_list {
        assert!(ctx.consensus.verify_pochm(blk, &pochm));
        assert!(pochm.vec.len() <= (FINALITY_DEPTH as f64).log2() as usize)
    }
    for (rec, tx_id) in receipts {
        assert!(ctx.consensus.verify_tx_receipt(&rec));
        // sanity check
        assert_eq!(rec.tracked_tx_id, tx_id);
    }

    for (proof, _) in pops {
        assert!(ctx.consensus.verify_proof_of_pub(&proof));
    }
}
#[tokio::test]
async fn test_receipts_in_random() {
    /*The test generates a random dag and builds blocks from it, with some modifications to the dag to make it more realistic
     whenever a new posterity block is reached by some margin, a batch of receipts is attempted to be generated for old blocks.
    These receipts are then verified at the end of the test.
    Occasionally the test fails because the real posterity of a block will change after a receipt for it has been pulled:
    the security margin should be enough for real data but test data behaves unexpectedly and thus this error persists despite attempts to mitigate it
    This error appears to decrease with higher BPS
    BPS too high though (10BPS) tends to result in block status errors

    Perhaps this test needs just be replaced by a simulation on real data.
     */
    const FINALITY_DEPTH: usize = 10;
    const DAG_SIZE: u64 = 500;
    const BPS: f64 = 5.0;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 50; //  is probably enough to avoid errors
            p.mergeset_size_limit = 30;
            p.finality_depth = FINALITY_DEPTH as u64;
            p.pruning_depth = (FINALITY_DEPTH * 3) as u64;
        })
        .build();
    let mut receipts1 = std::collections::HashMap::<_, _>::new();
    let mut receipts2 = std::collections::HashMap::<_, _>::new();
    let mut receipts3: HashMap<_, _> = std::collections::HashMap::<_, _>::new();

    let mut pops = std::collections::HashMap::<_, _>::new();
    let ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut unreceipted_blocks = vec![];
    let mut unproved_blocks = vec![];
    let mut pochms_list = vec![];
    let dag = generate_complex_dag(2.0, BPS, DAG_SIZE);
    eprintln!("{dag:?}");
    let mut next_posterity_score = FINALITY_DEPTH as u64;
    let mut mapper = HashMap::new();
    mapper.insert(dag.0, genesis_hash);
    /*
       A loop over the simulated dag is created, converting it step by step to a blockDag
       Mapper is the hash map coupling the abstract nodes on the dag object to the block hashes.
       Some of the vertices and nodes of the original node are changed so the blockdag behaves more realistically
    */
    for (ind, parents_ind) in dag.1.into_iter() {
        let mut parents = vec![];
        for par in parents_ind.clone().iter() {
            if let Some(&par_mapped) = mapper.get(par) {
                //avoid pointing on pending blocks, as pointing on them propogates the pending status forwards making the test meaningless
                if [BlockStatus::StatusUTXOValid, BlockStatus::StatusDisqualifiedFromChain]
                    .contains(&ctx.consensus.block_status(par_mapped))
                {
                    parents.push(par_mapped);
                }
            }
        }
        // make sure not all parents have been removed, if so ignore this node and do not attempt to convert it to a block.
        if parents.is_empty() {
            {
                continue;
            }
        }

        //add the new block to the blockdag, and update it on mapper
        let blk_hash = ind.into();
        ctx.add_utxo_valid_block_with_parents(blk_hash, parents, vec![]).await;
        mapper.insert(ind, blk_hash);
        /*periodically check if a new posterity point has been reached
        if so, attempt to create and store receipts and POPs for blocks pending for it.
        */
        if ctx.consensus.is_posterity_reached(next_posterity_score) {
            next_posterity_score += FINALITY_DEPTH as u64;

            //try and get proofs of publications for old blocks
            unproved_blocks.retain(|&old_block| {
                if ctx.consensus.block_transactions_store.get(old_block).is_err()
                    || !ctx
                        .consensus
                        .reachability_service()
                        .is_dag_ancestor_of_result(ctx.consensus.pruning_point(), old_block)
                        .unwrap_or(false)
                    || !ctx
                        .consensus
                        .reachability_service()
                        .is_dag_ancestor_of_result(old_block, ctx.consensus.get_sink())
                        .unwrap_or(false)
                {
                    // remove blocks with pruned data
                    return false;
                }
                let blk_header = ctx.consensus.get_header(old_block).unwrap();
                let pub_tx = ctx.consensus.block_transactions_store.get(old_block).unwrap()[0].id();
                let proof = ctx.consensus.generate_proof_of_pub(pub_tx, Some(blk_header.hash), None);
                if let Ok(proof) = proof {
                    pops.insert(old_block, proof);
                    true
                } else {
                    //proofs can fail because data is too new or because it is too old: discern the two cases
                    if pops.contains_key(&old_block) {
                        false
                    }
                    //there already is a key, don't touch it and say farewell
                    else {
                        true
                    } // wait for next iteration
                }
            });
            //try and get receipts and  pochms for old blocks
            unreceipted_blocks.retain(|&old_block| {
                if !ctx.consensus.reachability_service().is_chain_ancestor_of(ctx.consensus.pruning_point(), old_block)
                    || !ctx.consensus.reachability_service().is_chain_ancestor_of(old_block, ctx.consensus.get_sink())
                {
                    return false;
                }
                let blk_header = ctx.consensus.get_header(old_block).unwrap();

                let blk_pochm = ctx.consensus.generate_pochm(old_block);
                if blk_pochm.is_ok() {
                    pochms_list.push((blk_pochm.unwrap(), old_block));
                    let acc_tx =
                        ctx.consensus.acceptance_data_store.get(old_block).unwrap()[0].accepted_transactions[0].transaction_id;

                    receipts1.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, Some(blk_header.hash), None).unwrap());
                    receipts2.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, None, Some(blk_header.timestamp)).unwrap());
                    receipts3.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap());
                    true
                } else {
                    //proofs can fail because data is inexistent or because it is too old: discern the two cases
                    if receipts1.contains_key(&old_block) {
                        false
                    }
                    //there already is a key, don't touch it and say farewell
                    else {
                        true
                    } // wait for next iteration
                }
            });
        }
        //add the current block to the pending for proofs list
        unreceipted_blocks.push(blk_hash);
        unproved_blocks.push(blk_hash);
    }

    for blk in ctx.consensus.services.reachability_service.default_backward_chain_iterator(ctx.consensus.get_sink()) {
        eprintln!("chain blk: {:?}", blk);
    }
    for point in ctx.consensus.pruning_point_headers().into_iter() {
        eprintln!("posterity hash:{:?}\n bscore: {:?}", point.hash, point.blue_score);
    }
    eprintln!("receipts:{}", pops.len());
    assert!(receipts1.len() > DAG_SIZE as usize / (4.0 * BPS) as usize); //sanity check
    assert!(pops.len() > DAG_SIZE as usize / (BPS / 2.0).max(4.0) as usize); //sanity check
    for (pochm, blk) in pochms_list.into_iter() {
        eprintln!("blk_verified: {:?}", blk);
        assert!(ctx.consensus.verify_pochm(blk, &pochm));
        assert!(pochm.vec.len() <= (FINALITY_DEPTH as f64).log2() as usize);
    }
    for rec in receipts1.values() {
        assert!(ctx.consensus.verify_tx_receipt(rec));
    }
    for rec in receipts2.values() {
        assert!(ctx.consensus.verify_tx_receipt(rec));
    }
    for rec in receipts3.values() {
        assert!(ctx.consensus.verify_tx_receipt(rec));
    }

    for proof in pops.values() {
        assert!(ctx.consensus.verify_proof_of_pub(proof));
    }
}
