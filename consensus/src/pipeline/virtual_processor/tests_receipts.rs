use crate::{
    consensus::test_consensus::TestConsensus,
    model::stores::{
        acceptance_data::AcceptanceDataStoreReader, block_transactions::BlockTransactionsStoreReader, headers::HeaderStoreReader,
    },
    pipeline::virtual_processor::tests_util::TestContext,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    config::{params::MAINNET_PARAMS, ConfigBuilder},
};

#[tokio::test]
async fn test_chain_posterities() {
    const PERIODS: usize = 5;
    const FINALITY_DEPTH: usize = 20;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.finality_depth = FINALITY_DEPTH as u64;
            p.target_time_per_block = 50;
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
