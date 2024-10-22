use crate::{consensus::test_consensus::TestConsensus, pipeline::virtual_processor::tests_util::TestContext};
use kaspa_consensus_core::{
    api::ConsensusApi,
    config::{params::MAINNET_PARAMS, ConfigBuilder},
};
use kaspa_hashes::Hash;

#[tokio::test]
async fn test_chain_posterities() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.finality_depth = 10;
            p.target_time_per_block = 50;
            p.pruning_depth = 28;
        })
        .build();
    let mut expected_posterities: Vec<Hash> = vec![];
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut tip = genesis_hash; //compulsory initialization

    // mine enough blocks to reach first pruning point
    expected_posterities.push(genesis_hash);
    for _ in 0..3 {
        for _ in 0..10 {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
    }
    //check genesis behavior
    let pre_posterity = ctx.merkle_proofs_manager().get_pre_posterity_block_by_hash(genesis_hash);
    let post_posterity = ctx.merkle_proofs_manager().get_post_posterity_block(genesis_hash);
    assert_eq!(pre_posterity, expected_posterities[0]);
    assert_eq!(post_posterity.unwrap(), expected_posterities[1]);
    assert!(ctx.merkle_proofs_manager().verify_post_posterity_block(genesis_hash, expected_posterities[1]));

    let mut it = ctx.consensus.services.reachability_service.forward_chain_iterator(genesis_hash, tip, true).skip(1);
    for i in 0..27 {
        for _ in 0..9 {
            /*This loop:
            A) creates a new block and adds it at the tip
            B) validates the posterity qualitiesof the block 30 blocks in its past*/
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
            let block = it.next().unwrap();
            let pre_posterity = ctx.merkle_proofs_manager().get_pre_posterity_block_by_hash(block);
            let post_posterity = ctx.merkle_proofs_manager().get_post_posterity_block(block);
            assert_eq!(pre_posterity, expected_posterities[i]);
            assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
            assert!(ctx.merkle_proofs_manager().verify_post_posterity_block(block, expected_posterities[i + 1]));
        }
        //insert and update the next posterity block
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
        //verify posterity qualities of a 30 blocks in the past posterity block
        let past_posterity_block = it.next().unwrap();
        let pre_posterity = ctx.merkle_proofs_manager().get_pre_posterity_block_by_hash(past_posterity_block);
        let post_posterity = ctx.merkle_proofs_manager().get_post_posterity_block(past_posterity_block);
        assert_eq!(pre_posterity, expected_posterities[i]);
        assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
        assert!(ctx.merkle_proofs_manager().verify_post_posterity_block(past_posterity_block, expected_posterities[i + 2]));
        //update the iterator
        it = ctx.consensus.services.reachability_service.forward_chain_iterator(past_posterity_block, tip, true).skip(1);
    }

    for _ in 0..5 {
        //insert an extra few blocks, not enough for a new posterity
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    //check remaining blocks, which were yet to be pruned
    for i in 27..31 {
        for block in it.by_ref().take(9) {
            let pre_posterity = ctx.merkle_proofs_manager().get_pre_posterity_block_by_hash(block);
            let post_posterity = ctx.merkle_proofs_manager().get_post_posterity_block(block);

            assert_eq!(pre_posterity, expected_posterities[i]);
            if i == 30 {
                assert!(post_posterity.is_err());
            } else {
                assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
                assert!(ctx.merkle_proofs_manager().verify_post_posterity_block(block, expected_posterities[i + 1]));
            }
        }
        if i == 30 {
            break;
        }
        let past_posterity_block = it.next().unwrap();
        let pre_posterity = ctx.merkle_proofs_manager().get_pre_posterity_block_by_hash(past_posterity_block);
        let post_posterity = ctx.merkle_proofs_manager().get_post_posterity_block(past_posterity_block);
        assert_eq!(pre_posterity, expected_posterities[i]);
        //edge case logic
        if i == 29 {
            assert!(post_posterity.is_err());
        } else {
            assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
        }
    }

    for block in it {
        //check final blocks
        let pre_posterity = ctx.merkle_proofs_manager().get_pre_posterity_block_by_hash(block);
        let post_posterity = ctx.merkle_proofs_manager().get_post_posterity_block(block);
        assert_eq!(pre_posterity, expected_posterities[30]);
        assert!(post_posterity.is_err());
    }
}
