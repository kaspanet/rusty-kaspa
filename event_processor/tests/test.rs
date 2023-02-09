use std::sync::Arc;

use async_channel::unbounded;
use consensus::{
    config::Config,
    consensus::test_consensus::{create_temp_db, TestConsensus},
    params::DEVNET_PARAMS,
    test_helpers::*,
};
use consensus_core::{
    events::{BlockAddedEvent, ConsensusEvent, NewBlockTemplateEvent, VirtualChangeSetEvent},
    utxo::{
        utxo_collection::{UtxoCollection, UtxoCollectionExtensions},
        utxo_diff::UtxoDiff,
    },
};
use event_processor::{
    notify::{
        Notification, UtxosChangedNotification, VirtualDaaScoreChangedNotification, VirtualSelectedParentBlueScoreChangedNotification,
        VirtualSelectedParentChainChangedNotification,
    },
    processor::EventProcessor,
};
use rand::Rng;
use utxoindex::{api::DynUtxoIndexControlerApi, UtxoIndex};

//TODO: rewrite with simnet, when possible.
#[cfg(test)]
#[tokio::test]
async fn test_virtual_change_set_event() {
    let (test_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, test_recv) = unbounded::<Notification>();

    //TODO: testing with utxoindex requires at least genesis hash virtual parents store, for intial utxoindex reset
    /*
    let dummy_consensus_db = create_temp_db();
    let utxoindex_db = create_temp_db();

    let config = Config::new(DEVNET_PARAMS);

    let dummy_test_consensus = Arc::new(TestConsensus::new(dummy_consensus_db.1, &config, test_send.clone()));
    let utxoindex: DynUtxoIndexControlerApi = Arc::new(Some(Box::new(UtxoIndex::new(dummy_test_consensus, utxoindex_db.1))));
    */

    let event_processor = EventProcessor::new(Arc::new(None), consensus_recv, event_processor_send);

    tokio::spawn(async move { event_processor.run().await }); //run processor

    let rng = &mut rand::thread_rng();

    let mut to_add_collection = UtxoCollection::new();
    let mut to_remove_collection = UtxoCollection::new();
    for _ in 0..2 {
        to_add_collection.insert(generate_random_outpoint(&mut rng.clone()), generate_random_utxo(&mut rng.clone()));
        to_remove_collection.insert(generate_random_outpoint(&mut rng.clone()), generate_random_utxo(&mut rng.clone()));
    }

    let test_event = Arc::new(VirtualChangeSetEvent {
        utxo_diff: Arc::new(UtxoDiff { add: to_add_collection, remove: to_remove_collection }),
        parents: Arc::new(generate_random_hashes(&mut rng.clone(), 2)),
        selected_parent_blue_score: rng.gen(),
        daa_score: rng.gen(),
        mergeset_blues: Arc::new(generate_random_hashes(&mut rng.clone(), 2)),
        mergeset_reds: Arc::new(generate_random_hashes(&mut rng.clone(), 2)),
        accepted_tx_ids: Arc::new(generate_random_hashes(rng, 2)),
    });

    test_send.send(ConsensusEvent::VirtualChangeSet(test_event.clone())).await.expect("expected send");

    let mut virtual_selected_parent_blue_score_changed_count = 0;
    let mut virtual_selected_parent_chain_changed = 0;
    let mut virtual_daa_score_changed = 0;

    for _ in 0..3 {
        //We expect 3 notifications.
        match test_recv.recv().await.expect("expected recv") {
            //for now see utxoindex tests at `indexes::utxoindex::test`, testing with event processor is ommitted.
            Notification::VirtualSelectedParentBlueScoreChanged(virtual_selected_parent_blue_score_notification) => {
                assert_eq!(
                    test_event.selected_parent_blue_score,
                    virtual_selected_parent_blue_score_notification.virtual_selected_parent_blue_score
                );
                virtual_selected_parent_blue_score_changed_count += 1;
            }
            Notification::VirtualSelectedParentChainChanged(virtual_selected_parent_chain_changed_notification) => {
                assert_eq!(
                    test_event.mergeset_blues.len(),
                    virtual_selected_parent_chain_changed_notification.added_chain_block_hashes.len()
                );
                (0..test_event.mergeset_blues.len()).for_each(|i| {
                    assert_eq!(
                        test_event.mergeset_blues[i],
                        virtual_selected_parent_chain_changed_notification.added_chain_block_hashes[i]
                    );
                });

                assert_eq!(
                    test_event.mergeset_reds.len(),
                    virtual_selected_parent_chain_changed_notification.removed_chain_block_hashes.len()
                );
                (0..test_event.mergeset_reds.len()).for_each(|i| {
                    assert_eq!(
                        test_event.mergeset_reds[i],
                        virtual_selected_parent_chain_changed_notification.removed_chain_block_hashes[i]
                    );
                });

                assert_eq!(
                    test_event.accepted_tx_ids.len(),
                    virtual_selected_parent_chain_changed_notification.accepted_transaction_ids.len()
                );
                (0..test_event.accepted_tx_ids.len()).for_each(|i| {
                    assert_eq!(
                        test_event.accepted_tx_ids[i],
                        virtual_selected_parent_chain_changed_notification.accepted_transaction_ids[i]
                    );
                });

                virtual_selected_parent_chain_changed += 1;
            }
            Notification::VirtualDaaScoreChanged(virtual_daa_score_changed_notification) => {
                assert_eq!(test_event.daa_score, virtual_daa_score_changed_notification.virtual_daa_score);
                virtual_daa_score_changed += 1;
            }

            unexpected_notification => panic!("Unexpected notification: {:?}", unexpected_notification),
        }
    }

    assert!(test_recv.is_empty()); //assert we have no extra messages pending

    //assert we got no double notifications
    assert_eq!(virtual_selected_parent_blue_score_changed_count, 1);
    assert_eq!(virtual_selected_parent_chain_changed, 1);
    assert_eq!(virtual_daa_score_changed, 1);
}

#[cfg(test)]
#[tokio::test]
async fn test_block_added_event() {
    let (test_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, test_recv) = unbounded::<Notification>();
    let event_processor = EventProcessor::new(Arc::new(None), consensus_recv, event_processor_send);

    let rng = &mut rand::thread_rng();

    let test_event = Arc::new(BlockAddedEvent { block: generate_random_block(&mut rng, 2, 2, 2, 2) });

    tokio::spawn(async move { event_processor.run().await }); //run processor

    test_send.send(ConsensusEvent::BlockAdded(test_event.clone())).await.expect("expected send");

    match test_recv.recv().await.expect("expected recv") {
        Notification::BlockAdded(block_added_notification) => {
            //TODO: create `assert_eq_[kaspa_sturct]` helper macros in `consensus::test_helpers` to avoid this.

            assert_eq!(block_added_notification.block.header.hash, test_event.block.header.hash);
            assert_eq!(block_added_notification.block.header.version, test_event.block.header.version);
            assert_eq!(block_added_notification.block.header.parents_by_level.len(), test_event.block.header.parents_by_level.len());
            (0..block_added_notification.block.header.parents_by_level.len()).for_each(|i| {
                assert_eq!(
                    block_added_notification.block.header.parents_by_level[i].len(),
                    test_event.block.header.parents_by_level[i].len()
                )(0..block_added_notification.block.header.parents_by_level[i].len())
                .for_each(|i2| {
                    assert_eq!(
                        block_added_notification.block.header.parents_by_level[i][i2],
                        test_event.block.header.parents_by_level[i][i2]
                    )
                });
            });
            assert_eq!(block_added_notification.block.header.hash_merkle_root, test_event.block.header.hash_merkle_root);
            assert_eq!(block_added_notification.block.header.accepted_id_merkle_root, test_event.block.header.accepted_id_merkle_root);
            assert_eq!(block_added_notification.block.header.utxo_commitment, test_event.block.header.utxo_commitment);
            assert_eq!(block_added_notification.block.header.timestamp, test_event.block.header.timestamp);
            assert_eq!(block_added_notification.block.header.bits, test_event.block.header.bits);
            assert_eq!(block_added_notification.block.header.nonce, test_event.block.header.nonce);
            assert_eq!(block_added_notification.block.header.daa_score, test_event.block.header.daa_score);
            assert_eq!(block_added_notification.block.header.blue_work, test_event.block.header.blue_work);
            assert_eq!(block_added_notification.block.header.blue_score, test_event.block.header.blue_score);
            assert_eq!(block_added_notification.block.header.pruning_point, test_event.block.header.pruning_point);

            assert_eq!(block_added_notification.block.transactions.len(), test_event.block.transactions.len());
            (0..block_added_notification.block.transactions.len()).for_each(|i| {
                assert_eq!(block_added_notification.block.transactions[i].id(), test_event.block.transactions[i].id());
                assert_eq!(block_added_notification.block.transactions[i].version, test_event.block.transactions[i].version);
                assert_eq!(block_added_notification.block.transactions[i].lock_time, test_event.block.transactions[i].lock_time);
                assert_eq!(
                    block_added_notification.block.transactions[i].subnetwork_id,
                    test_event.block.transactions[i].subnetwork_id
                );
                assert_eq!(block_added_notification.block.transactions[i].gas, test_event.block.transactions[i].gas);
                assert_eq!(
                    block_added_notification.block.transactions[i].payload.as_slice(),
                    test_event.block.transactions[i].payload.as_slice()
                );
                assert_eq!(block_added_notification.block.transactions[i].inputs.len(), test_event.block.transactions[i].inputs.len());
                (0..block_added_notification.block.transactions[i].inputs.len()).for_each(|i2| {
                    assert_eq!(
                        block_added_notification.block.transactions[i].inputs[i2].previous_outpoint.transaction_id,
                        test_event.block.transactions[i].inputs[i2].previous_outpoint.transaction_id
                    );
                    assert_eq!(
                        block_added_notification.block.transactions[i].inputs[i2].previous_outpoint.index,
                        test_event.block.transactions[i].inputs[i2].previous_outpoint.index
                    );
                    assert_eq!(
                        block_added_notification.block.transactions[i].inputs[i2].signature_script.as_slice(),
                        test_event.block.transactions[i].inputs[i2].signature_script.as_slice()
                    );
                    assert_eq!(
                        block_added_notification.block.transactions[i].inputs[i2].sequence,
                        test_event.block.transactions[i].inputs[i2].sequence
                    );
                    assert_eq!(
                        block_added_notification.block.transactions[i].inputs[i2].sig_op_count,
                        test_event.block.transactions[i].inputs[i2].sig_op_count
                    );
                });
                assert_eq!(
                    block_added_notification.block.transactions[i].outputs.len(),
                    test_event.block.transactions[i].outputs.len()
                );
                (0..block_added_notification.block.transactions[i].outputs.len()).for_each(|i2| {
                    assert_eq!(
                        block_added_notification.block.transactions[i].outputs[i2].value,
                        test_event.block.transactions[i].outputs[i2].value
                    );
                    assert_eq!(
                        block_added_notification.block.transactions[i].outputs[i2].script_public_key.version(),
                        test_event.block.transactions[i].outputs[i2].script_public_key.version()
                    );
                    assert_eq!(
                        block_added_notification.block.transactions[i].outputs[i2].script_public_key.script(),
                        test_event.block.transactions[i].outputs[i2].script_public_key.script()
                    );
                });
            });
        }
        unexpected_notification => panic!("Unexpected notification: {:?}", unexpected_notification),
    }

    assert!(test_recv.is_empty()); //assert we have no extra messages pending
}

#[cfg(test)]
#[tokio::test]
async fn test_new_block_template_event() {
    let (test_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, test_recv) = unbounded::<Notification>();
    let event_processor = EventProcessor::new(Arc::new(None), consensus_recv, event_processor_send);

    let rng = &mut rand::thread_rng();

    let test_event = Arc::new(NewBlockTemplateEvent {});

    tokio::spawn(async move { event_processor.run().await }); //run processor

    test_send.send(ConsensusEvent::NewBlockTemplate(test_event.clone())).await.expect("expected send");

    match test_recv.recv().await.expect("expected recv") {
        Notification::NewBlockTemplate(_) => (),
        unexpected_notification => panic!("Unexpected notification: {:?}", unexpected_notification),
    }

    assert!(test_recv.is_empty());
}

#[cfg(test)]
#[tokio::test]
async fn test_finality_conflict_event() {
    let (test_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, test_recv) = unbounded::<Notification>();
    let event_processor = EventProcessor::new(Arc::new(None), consensus_recv, event_processor_send);

    let rng = &mut rand::thread_rng();
    let test_event = Arc::new(NewBlockTemplateEvent {});

    tokio::spawn(async move { event_processor.run().await }); //run processor

    test_send.send(ConsensusEvent::FinalityConflict(test_event.clone())).await.expect("expected send");

    match test_recv.recv().await.expect("expected recv") {
        Notification::FinalityConflict(_) => (),
        unexpected_notification => panic!("Unexpected notification: {:?}", unexpected_notification),
    }

    assert!(test_recv.is_empty());
}

#[cfg(test)]
#[tokio::test]
async fn test_finality_conflict_resolved_event() {
    let (test_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, test_recv) = unbounded::<Notification>();
    let event_processor = EventProcessor::new(Arc::new(None), consensus_recv, event_processor_send);

    let rng = &mut rand::thread_rng();

    let test_event = Arc::new(NewBlockTemplateEvent {});

    tokio::spawn(async move { event_processor.run().await }); //run processor

    test_send.send(ConsensusEvent::FinalityConflictResolved(test_event.clone())).await.expect("expected send");

    match test_recv.recv().await.expect("expected recv") {
        Notification::FinalityConflictResolved(_) => (),
        unexpected_notification => panic!("Unexpected notification: {:?}", unexpected_notification),
    }

    assert!(test_recv.is_empty());
}

#[cfg(test)]
#[tokio::test]
async fn test_pruning_point_utxo_set_override_event() {
    let (test_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, test_recv) = unbounded::<Notification>();
    let event_processor = EventProcessor::new(Arc::new(None), consensus_recv, event_processor_send);

    //TODO: testing with utxoindex requires at least genesis hash virtual parents store, for intial utxoindex reset
    /*
    let dummy_consensus_db = create_temp_db();
    let utxoindex_db = create_temp_db();

    let config = Config::new(DEVNET_PARAMS);

    let dummy_test_consensus = Arc::new(TestConsensus::new(dummy_consensus_db.1, &config, test_send.clone()));
    let utxoindex: DynUtxoIndexControlerApi = Arc::new(Some(Box::new(UtxoIndex::new(dummy_test_consensus, utxoindex_db.1))));
    */

    let rng = &mut rand::thread_rng();

    let test_event = Arc::new(NewBlockTemplateEvent {});

    tokio::spawn(async move { event_processor.run().await }); //run processor

    test_send.send(ConsensusEvent::PruningPointUTXOSetOverride(test_event.clone())).await.expect("expected send");

    match test_recv.recv().await.expect("expected recv") {
        Notification::PruningPointUTXOSetOverride(_) => (),
        unexpected_notification => panic!("Unexpected notification: {:?}", unexpected_notification),
    }

    assert!(test_recv.is_empty());
}
