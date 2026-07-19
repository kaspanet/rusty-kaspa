use std::{collections::HashMap, thread::sleep, time::Duration};

use super::tests_util::TestContext;
use crate::{
    consensus::test_consensus::TestConsensus,
    model::stores::{
        acceptance_data::AcceptanceDataStoreReader, pruning::PruningStoreReader, selected_chain::SelectedChainStoreReader,
    },
    processes::reachability::tests::r#gen::generate_complex_dag,
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    blockstatus::BlockStatus,
    config::{
        ConfigBuilder,
        params::{ForkActivation, MAINNET_PARAMS},
    },
    receipts::TxReceipt,
    subnets::SubnetworkId,
    tx::Transaction,
};
use kaspa_hashes::Hash;

const LANE_PARENT_REFS_FINALITY_DEPTH: usize = 4;
const MIXED_LANE_BATCH_TXS: usize = 17;
const PRUNING_WAIT_ATTEMPTS: usize = 100;

#[tokio::test]
async fn test_receipts_in_chain() {
    const PERIODS: usize = 5;
    const FINALITY_DEPTH: usize = 20;
    const BPS: f64 = 10.0;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 10;
            p.mergeset_size_limit = 30;
            p.ghostdag_k = 4;
            p.finality_depth = FINALITY_DEPTH as u64;
            p.pruning_depth = (FINALITY_DEPTH * 3 - 5) as u64;
            p.toccata_activation = ForkActivation::always();
            p.target_time_per_block = (1000.0 / BPS) as u64;
        })
        .build();
    let mut expected_posterities = vec![];
    let mut receipts = vec![];

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut tip = genesis_hash;

    expected_posterities.push(genesis_hash);
    for _ in 0..3 {
        for _ in 0..FINALITY_DEPTH {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        }
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
    }

    assert_posterity(&ctx, genesis_hash, expected_posterities[0], Some(expected_posterities[1]));

    let mut chain_cursor = genesis_hash;
    for i in 0..PERIODS - 3 {
        for _ in 0..FINALITY_DEPTH - 1 {
            ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
            let block = ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1).unwrap();
            chain_cursor = block;
            push_receipt_for_accepted_tx(&ctx, &mut receipts, block);
            assert_posterity(&ctx, block, expected_posterities[i], Some(expected_posterities[i + 1]));
        }
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
        ctx.assert_tips_num(1);
        tip = ctx.consensus.get_tips()[0];
        expected_posterities.push(tip);
        let past_posterity_block =
            ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1).unwrap();
        chain_cursor = past_posterity_block;
        push_receipt_for_accepted_tx(&ctx, &mut receipts, past_posterity_block);
        assert_posterity(&ctx, past_posterity_block, expected_posterities[i], Some(expected_posterities[i + 2]));
    }

    for _ in 0..FINALITY_DEPTH / 2 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }

    for i in PERIODS - 3..PERIODS + 1 {
        for _ in 0..FINALITY_DEPTH - 1 {
            let Some(block) = ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1)
            else {
                break;
            };
            chain_cursor = block;
            if i == PERIODS {
                assert_posterity(&ctx, block, expected_posterities[i], None);
            } else {
                assert_posterity(&ctx, block, expected_posterities[i], Some(expected_posterities[i + 1]));
            }
        }
        if i == PERIODS {
            break;
        }
        let past_posterity_block =
            ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1).unwrap();
        chain_cursor = past_posterity_block;
        if i == PERIODS - 1 {
            assert_posterity(&ctx, past_posterity_block, expected_posterities[i], None);
        } else {
            assert_posterity(&ctx, past_posterity_block, expected_posterities[i], Some(expected_posterities[i + 2]));
        }
    }

    while let Some(block) = ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1) {
        chain_cursor = block;
        assert_posterity(&ctx, block, expected_posterities[PERIODS], None);
    }

    assert_tracked_receipts_verify(&ctx, &receipts, "in chain receipt test");
}

#[tokio::test]
async fn test_receipts_in_random() {
    const FINALITY_DEPTH: usize = 10;
    const DAG_SIZE: u64 = 500;
    const BPS: f64 = 10.0;
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 10;
            p.mergeset_size_limit = 30;
            p.ghostdag_k = 4;
            p.finality_depth = FINALITY_DEPTH as u64;
            p.pruning_depth = (FINALITY_DEPTH * 3 - 5) as u64;
            p.toccata_activation = ForkActivation::always();
        })
        .build();
    let mut receipts1 = HashMap::<_, _>::new();
    let mut receipts2 = HashMap::<_, _>::new();
    let mut receipts3 = HashMap::<_, _>::new();

    let ctx = TestContext::new(TestConsensus::new(&config));
    let genesis_hash = ctx.consensus.params().genesis.hash;
    let mut posterity_list = vec![genesis_hash];

    let dag = generate_complex_dag(2.0, BPS, DAG_SIZE);
    let mut next_posterity_score = FINALITY_DEPTH as u64;
    let mut mapper = HashMap::new();
    mapper.insert(dag.0, genesis_hash);

    for (ind, parents_ind) in dag.1.into_iter() {
        let mut parents = vec![];
        for par in parents_ind.clone().iter() {
            if let Some(&par_mapped) = mapper.get(par)
                && [BlockStatus::StatusUTXOValid, BlockStatus::StatusDisqualifiedFromChain]
                    .contains(&ctx.consensus.block_status(par_mapped))
            {
                parents.push(par_mapped);
            }
        }
        if parents.is_empty() {
            continue;
        }

        mapper.insert(ind, ind.into());

        ctx.add_utxo_valid_block_with_parents(mapper[&ind], parents, vec![]).await;
        if ctx.consensus.is_posterity_reached(next_posterity_score) {
            sleep(Duration::from_millis(300));
            wait_for_retention_checkpoint_to_match_root(&ctx);
            next_posterity_score += FINALITY_DEPTH as u64;
            posterity_list.push(ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(ctx.consensus.get_sink()));
            if posterity_list.len() >= 3 {
                for old_block in ctx
                    .consensus
                    .services
                    .reachability_service
                    .forward_chain_iterator(ctx.consensus.pruning_point(), posterity_list[posterity_list.len() - 2], true)
                    .skip(1)
                {
                    let blk_header = ctx.consensus.get_header(old_block).unwrap();
                    if old_block != genesis_hash && ctx.consensus.selected_chain_store.read().get_by_hash(old_block).is_ok() {
                        let acc_tx = accepted_tx_id(&ctx, old_block);
                        receipts1.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, Some(blk_header.hash), None).unwrap());
                        receipts2
                            .insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, None, Some(blk_header.timestamp)).unwrap());

                        receipts3.insert(old_block, ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap());
                    }
                }
            }
        }
    }

    assert!(receipts1.len() >= DAG_SIZE as usize / (4.5 * BPS) as usize);

    assert_receipt_values_verify(&ctx, receipts1.values(), "with accepting-block hint");
    assert_receipt_values_verify(&ctx, receipts2.values(), "with timestamp hint");
    assert_receipt_values_verify(&ctx, receipts3.values(), "without hints");
}

#[tokio::test]
async fn test_receipts_across_lane_parent_refs_and_posterity_updates() {
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.ghostdag_k = 4;
            p.finality_depth = LANE_PARENT_REFS_FINALITY_DEPTH as u64;
            p.pruning_depth = (LANE_PARENT_REFS_FINALITY_DEPTH * 4) as u64;
            p.blockrate.coinbase_maturity = 0;
            p.toccata_activation = ForkActivation::always();
        })
        .build();

    let mut ctx = TestContext::new(TestConsensus::new(&config));
    let lanes = ReceiptTestLanes::new();
    let genesis = ctx.consensus.params().genesis.hash;

    let b1 = block_hash(1);
    ctx.add_op_true_block(b1, genesis, vec![]).await;
    let b2 = block_hash(2);
    ctx.add_op_true_block(b2, b1, vec![]).await;

    let tx_active_1 = ctx.spend_coinbase_output(b2, lanes.active, vec![1]);
    let b3 = block_hash(3);
    ctx.add_op_true_block(b3, b2, vec![tx_active_1]).await;

    let tx_active_2 = ctx.spend_coinbase_output(b3, lanes.active, vec![2]);
    let tx_active_2_id = tx_active_2.id();
    let b4 = block_hash(4);
    ctx.add_op_true_block(b4, b3, vec![tx_active_2]).await;

    // This lane will later be touched at the one-block active-window boundary.
    let tx_boundary_old = ctx.spend_coinbase_output(b4, lanes.boundary, vec![3]);
    let b5 = block_hash(5);
    ctx.add_op_true_block(b5, b4, vec![tx_boundary_old]).await;

    // This lane will later be reactivated after it is outside both possible parent windows.
    let tx_expired_old = ctx.spend_coinbase_output(b5, lanes.expired, vec![4]);
    let b6 = block_hash(6);
    ctx.add_op_true_block(b6, b5, vec![tx_expired_old]).await;

    let b7 = block_hash(7);
    ctx.add_op_true_block(b7, b6, vec![]).await;
    let b8 = block_hash(8);
    ctx.add_op_true_block(b8, b7, vec![]).await;
    let b9 = block_hash(9);
    ctx.add_op_true_block(b9, b8, vec![]).await;

    // Accepted by b11. The old boundary-lane tip is at score 6. For b11, the
    // writer's active window is [7, 10], while the selected-parent POV window is [6, 10].
    let tx_boundary = ctx.spend_coinbase_output(b9, lanes.boundary, vec![5]);
    let tx_boundary_id = tx_boundary.id();
    let b10 = block_hash(10);
    ctx.add_op_true_block(b10, b9, vec![tx_boundary]).await;
    let b11 = block_hash(11);
    ctx.add_op_true_block(b11, b10, vec![]).await;

    let tip = add_empty_op_true_blocks(&mut ctx, b11, 12..=27).await;
    let mut funding_blocks = (12..=27).map(block_hash).collect::<Vec<_>>();
    funding_blocks.push(b11);

    // b29 accepts a large mixed lane batch from b28. This covers global merge_idx
    // across lanes, many same-lane leaves, and a wide-gap lane reactivation.
    let mixed_batch = build_mixed_lane_batch(&ctx, &funding_blocks, &lanes);

    let many_tx_block = block_hash(28);
    ctx.add_op_true_block(many_tx_block, tip, mixed_batch.txs).await;

    // Same-lane activity after the tracked many-tx block, before its posterity.
    let tx_a_update = ctx.spend_coinbase_output(many_tx_block, lanes.a, vec![16]);
    let accepting_many_block = block_hash(29);
    ctx.add_op_true_block(accepting_many_block, many_tx_block, vec![tx_a_update]).await;

    // Unrelated lane activity after the tracked many-tx block, before its posterity.
    let tx_noise = ctx.spend_coinbase_output(accepting_many_block, lanes.noise, vec![17]);
    let b30 = block_hash(30);
    ctx.add_op_true_block(b30, accepting_many_block, vec![tx_noise]).await;
    let b31 = block_hash(31);
    ctx.add_op_true_block(b31, b30, vec![]).await;
    let posterity_many_block = block_hash(32);
    ctx.add_op_true_block(posterity_many_block, b31, vec![]).await;

    let receipt_cases = [
        ReceiptCase::new("active lane reuses parent tip", tx_active_2_id, b5),
        ReceiptCase::new("boundary lane reactivation", tx_boundary_id, b11),
        ReceiptCase::new("expired lane reactivation", mixed_batch.expired_tx_id, accepting_many_block),
        ReceiptCase::new("interleaved lane A first tx", mixed_batch.a_first_tx_id, accepting_many_block),
        ReceiptCase::new("interleaved lane A second tx", mixed_batch.a_second_tx_id, accepting_many_block),
        ReceiptCase::new("interleaved lane B first tx", mixed_batch.b_first_tx_id, accepting_many_block),
        ReceiptCase::new("interleaved lane B second tx", mixed_batch.b_second_tx_id, accepting_many_block),
    ];

    let receipts = generate_receipts_for_cases(&ctx, &receipt_cases);
    assert_labeled_receipts_verify(&ctx, &receipts, "before pruning");

    let initial_retention_root = ctx.consensus.pruning_point_store.read().retention_period_root().unwrap();
    add_empty_op_true_blocks(&mut ctx, posterity_many_block, 33..=96).await;
    wait_for_retention_root_to_advance(&ctx, initial_retention_root);

    assert_labeled_receipts_verify(&ctx, &receipts, "after pruning");
}

fn block_hash(index: u64) -> Hash {
    Hash::from_u64_word(index)
}

fn lane(index: u8) -> SubnetworkId {
    SubnetworkId::from_namespace([0, 0, 0, index])
}

struct ReceiptTestLanes {
    active: SubnetworkId,
    boundary: SubnetworkId,
    expired: SubnetworkId,
    a: SubnetworkId,
    b: SubnetworkId,
    noise: SubnetworkId,
}

impl ReceiptTestLanes {
    fn new() -> Self {
        Self { active: lane(10), boundary: lane(20), expired: lane(30), a: lane(40), b: lane(41), noise: lane(60) }
    }
}

#[derive(Clone, Copy)]
struct ReceiptCase {
    label: &'static str,
    tx_id: Hash,
    accepting_block: Hash,
}

impl ReceiptCase {
    fn new(label: &'static str, tx_id: Hash, accepting_block: Hash) -> Self {
        Self { label, tx_id, accepting_block }
    }
}

struct MixedLaneBatch {
    txs: Vec<Transaction>,
    expired_tx_id: Hash,
    a_first_tx_id: Hash,
    a_second_tx_id: Hash,
    b_first_tx_id: Hash,
    b_second_tx_id: Hash,
}

struct TrackedReceipt {
    tx_id: Hash,
    receipt: TxReceipt,
}

async fn add_empty_op_true_blocks(ctx: &mut TestContext, mut tip: Hash, indices: impl IntoIterator<Item = u64>) -> Hash {
    for index in indices {
        let block = block_hash(index);
        ctx.add_op_true_block(block, tip, vec![]).await;
        tip = block;
    }
    tip
}

fn build_mixed_lane_batch(ctx: &TestContext, funding_blocks: &[Hash], lanes: &ReceiptTestLanes) -> MixedLaneBatch {
    assert_eq!(funding_blocks.len(), MIXED_LANE_BATCH_TXS);

    let mut txs = Vec::with_capacity(funding_blocks.len());
    let mut a_first_tx_id = None;
    let mut a_second_tx_id = None;
    let mut b_first_tx_id = None;
    let mut b_second_tx_id = None;
    let mut expired_tx_id = None;

    for (i, source_block) in funding_blocks.iter().copied().enumerate() {
        let tx_lane = match i {
            15 => lanes.expired,
            4 | 7 | 10 | 13 => lanes.noise,
            i if i % 2 == 0 => lanes.a,
            _ => lanes.b,
        };
        let tx = ctx.spend_coinbase_output(source_block, tx_lane, vec![i as u8]);
        match i {
            0 => a_first_tx_id = Some(tx.id()),
            1 => b_first_tx_id = Some(tx.id()),
            2 => a_second_tx_id = Some(tx.id()),
            3 => b_second_tx_id = Some(tx.id()),
            15 => expired_tx_id = Some(tx.id()),
            _ => {}
        }
        txs.push(tx);
    }

    MixedLaneBatch {
        txs,
        expired_tx_id: expired_tx_id.expect("mixed batch should include an expired-lane tx"),
        a_first_tx_id: a_first_tx_id.expect("mixed batch should include first lane A tx"),
        a_second_tx_id: a_second_tx_id.expect("mixed batch should include second lane A tx"),
        b_first_tx_id: b_first_tx_id.expect("mixed batch should include first lane B tx"),
        b_second_tx_id: b_second_tx_id.expect("mixed batch should include second lane B tx"),
    }
}

fn generate_receipts_for_cases(ctx: &TestContext, cases: &[ReceiptCase]) -> Vec<(ReceiptCase, TxReceipt)> {
    cases
        .iter()
        .map(|case| {
            let receipt = ctx.consensus.generate_tx_receipt(case.tx_id, Some(case.accepting_block), None).unwrap();
            (*case, receipt)
        })
        .collect()
}

fn assert_labeled_receipts_verify(ctx: &TestContext, receipts: &[(ReceiptCase, TxReceipt)], phase: &str) {
    let failed = receipts
        .iter()
        .filter_map(|(case, receipt)| (!ctx.consensus.verify_tx_receipt(receipt)).then_some(case.label))
        .collect::<Vec<_>>();
    assert!(failed.is_empty(), "receipt verification failed {phase} for lane cases: {}", failed.join(", "));
}

fn assert_receipt_values_verify<'a>(ctx: &TestContext, receipts: impl IntoIterator<Item = &'a TxReceipt>, phase: &str) {
    let failed = receipts.into_iter().filter(|receipt| !ctx.consensus.verify_tx_receipt(receipt)).count();
    assert_eq!(failed, 0, "{failed} receipts failed verification {phase}");
}

fn assert_tracked_receipts_verify(ctx: &TestContext, receipts: &[TrackedReceipt], phase: &str) {
    for tracked in receipts {
        assert_eq!(tracked.receipt.tracked_tx_id, tracked.tx_id);
    }
    assert_receipt_values_verify(ctx, receipts.iter().map(|tracked| &tracked.receipt), phase);
}

fn accepted_tx_id(ctx: &TestContext, block: Hash) -> Hash {
    ctx.consensus.acceptance_data_store.get(block).unwrap()[0].accepted_transactions[0].transaction_id
}

fn push_receipt_for_accepted_tx(ctx: &TestContext, receipts: &mut Vec<TrackedReceipt>, block: Hash) {
    let tx_id = accepted_tx_id(ctx, block);
    receipts.push(TrackedReceipt { tx_id, receipt: ctx.consensus.generate_tx_receipt(tx_id, None, None).unwrap() });
}

fn assert_posterity(ctx: &TestContext, block: Hash, expected_pre: Hash, expected_post: Option<Hash>) {
    let receipts_manager = &ctx.consensus.services.tx_receipts_manager;
    assert_eq!(receipts_manager.get_pre_posterity_block_by_hash(block), expected_pre);

    match expected_post {
        Some(post) => {
            assert_eq!(receipts_manager.get_post_posterity_block(block).unwrap(), post);
            assert!(receipts_manager.verify_post_posterity_block(block, post));
        }
        None => assert!(receipts_manager.get_post_posterity_block(block).is_err()),
    }
}

fn retention_checkpoint_and_root(ctx: &TestContext) -> (Hash, Hash) {
    let pruning_read = ctx.consensus.pruning_point_store.read();
    (pruning_read.retention_checkpoint().unwrap(), pruning_read.retention_period_root().unwrap())
}

fn wait_for_retention_checkpoint_to_match_root(ctx: &TestContext) -> Hash {
    for _ in 0..PRUNING_WAIT_ATTEMPTS {
        let (retention_checkpoint, retention_root) = retention_checkpoint_and_root(ctx);
        if retention_checkpoint == retention_root {
            return retention_root;
        }
        sleep(Duration::from_millis(100));
    }

    let (retention_checkpoint, retention_root) = retention_checkpoint_and_root(ctx);
    assert_eq!(retention_checkpoint, retention_root, "pruning did not settle before receipt reverification");
    retention_root
}

fn wait_for_retention_root_to_advance(ctx: &TestContext, initial_retention_root: Hash) {
    let retention_root = wait_for_retention_checkpoint_to_match_root(ctx);
    assert_ne!(retention_root, initial_retention_root, "filler blocks did not advance the retention root");
}
