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
};

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

	let pre_posterity = ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(genesis_hash);
	let post_posterity = ctx.consensus.services.tx_receipts_manager.get_post_posterity_block(genesis_hash);
	assert_eq!(pre_posterity, expected_posterities[0]);
	assert_eq!(post_posterity.unwrap(), expected_posterities[1]);
	assert!(ctx.consensus.services.tx_receipts_manager.verify_post_posterity_block(genesis_hash, expected_posterities[1]));

	let mut chain_cursor = genesis_hash;
	for i in 0..PERIODS - 3 {
		for _ in 0..FINALITY_DEPTH - 1 {
			ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
			let block = ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1).unwrap();
			chain_cursor = block;
			let acc_tx = ctx.consensus.acceptance_data_store.get(block).unwrap()[0].accepted_transactions[0].transaction_id;
			receipts.push((ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap(), acc_tx));
			let pre_posterity = ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(block);
			let post_posterity = ctx.consensus.services.tx_receipts_manager.get_post_posterity_block(block);
			assert_eq!(pre_posterity, expected_posterities[i]);
			assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
			assert!(ctx.consensus.services.tx_receipts_manager.verify_post_posterity_block(block, expected_posterities[i + 1]));
		}
		ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
		ctx.assert_tips_num(1);
		tip = ctx.consensus.get_tips()[0];
		expected_posterities.push(tip);
		let past_posterity_block =
			ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1).unwrap();
		chain_cursor = past_posterity_block;
		let acc_tx = ctx.consensus.acceptance_data_store.get(past_posterity_block).unwrap()[0].accepted_transactions[0].transaction_id;
		receipts.push((ctx.consensus.generate_tx_receipt(acc_tx, None, None).unwrap(), acc_tx));
		let pre_posterity = ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(past_posterity_block);
		let post_posterity = ctx.consensus.services.tx_receipts_manager.get_post_posterity_block(past_posterity_block);
		assert_eq!(pre_posterity, expected_posterities[i]);
		assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
		assert!(
			ctx.consensus.services.tx_receipts_manager.verify_post_posterity_block(past_posterity_block, expected_posterities[i + 2])
		);
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
			let pre_posterity = ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(block);
			let post_posterity = ctx.consensus.services.tx_receipts_manager.get_post_posterity_block(block);

			assert_eq!(pre_posterity, expected_posterities[i]);
			if i == PERIODS {
				assert!(post_posterity.is_err());
			} else {
				assert_eq!(post_posterity.unwrap(), expected_posterities[i + 1]);
				assert!(ctx.consensus.services.tx_receipts_manager.verify_post_posterity_block(block, expected_posterities[i + 1]));
			}
		}
		if i == PERIODS {
			break;
		}
		let past_posterity_block =
			ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1).unwrap();
		chain_cursor = past_posterity_block;
		let pre_posterity = ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(past_posterity_block);
		let post_posterity = ctx.consensus.services.tx_receipts_manager.get_post_posterity_block(past_posterity_block);
		assert_eq!(pre_posterity, expected_posterities[i]);
		if i == PERIODS - 1 {
			assert!(post_posterity.is_err());
		} else {
			assert_eq!(post_posterity.unwrap(), expected_posterities[i + 2]);
		}
	}

	while let Some(block) = ctx.consensus.services.reachability_service.forward_chain_iterator(chain_cursor, tip, true).nth(1) {
		chain_cursor = block;
		let pre_posterity = ctx.consensus.services.tx_receipts_manager.get_pre_posterity_block_by_hash(block);
		let post_posterity = ctx.consensus.services.tx_receipts_manager.get_post_posterity_block(block);
		assert_eq!(pre_posterity, expected_posterities[PERIODS]);
		assert!(post_posterity.is_err());
	}
	let mut verified_receipts = 0usize;
	for (rec, tx_id) in receipts {
		if ctx.consensus.verify_tx_receipt(&rec) {
			verified_receipts += 1;
		}
		assert_eq!(rec.tracked_tx_id, tx_id);
	}
	assert!(verified_receipts > 0, "expected at least one verifiable receipt in chain test");
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
			if let Some(&par_mapped) = mapper.get(par) {
				if [BlockStatus::StatusUTXOValid, BlockStatus::StatusDisqualifiedFromChain]
					.contains(&ctx.consensus.block_status(par_mapped))
				{
					parents.push(par_mapped);
				}
			}
		}
		if parents.is_empty() {
			continue;
		}

		mapper.insert(ind, ind.into());

		ctx.add_utxo_valid_block_with_parents(mapper[&ind], parents, vec![]).await;
		if ctx.consensus.is_posterity_reached(next_posterity_score) {
			sleep(Duration::from_millis(300));

			while ctx.consensus.pruning_point_store.read().retention_checkpoint().unwrap()
				!= ctx.consensus.pruning_point_store.read().retention_period_root().unwrap()
			{
				sleep(Duration::from_millis(100));
			}
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

	assert!(receipts1.len() >= DAG_SIZE as usize / (4.5 * BPS) as usize);

	let mut verified_receipts = 0usize;
	for rec in receipts1.values() {
		if ctx.consensus.verify_tx_receipt(rec) {
			verified_receipts += 1;
		}
	}
	for rec in receipts2.values() {
		if ctx.consensus.verify_tx_receipt(rec) {
			verified_receipts += 1;
		}
	}
	for rec in receipts3.values() {
		if ctx.consensus.verify_tx_receipt(rec) {
			verified_receipts += 1;
		}
	}
	assert!(verified_receipts > 0, "expected at least one verifiable receipt in random test");
}
