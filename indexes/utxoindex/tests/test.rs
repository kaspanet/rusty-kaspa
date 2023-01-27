use consensus::model::stores::utxo_set::UtxoSetStore;
use consensus::model::stores::virtual_state::VirtualStateStore;
use consensus::consensus::test_consensus::{create_temp_db, TestConsensus};
use consensus::model::stores::virtual_state::VirtualState;
use consensus::params::DEVNET_PARAMS;
use consensus_core::tx::TransactionOutpoint;
use consensus_core::utxo::utxo_diff::UtxoDiff;
use consensus_core::{api::ConsensusApi, utxo::utxo_collection::UtxoCollection};
use rand::Rng;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::test;
use utxoindex::test_helpers::VirtualChangeEmulator;
use utxoindex::{api::UtxoIndexApi, utxoindex::UtxoIndex};

/// This is an ad hoc and preliminary testing platform, using a virtual change emulator, to show that the utxoindex works.
/// Note: the emulator does not use consensus conform or valid stuctures.
/// TODO: use proper simnet when implemented.
#[test]
async fn test_utxoindex() {
    //set-up random number generator
    let mut rng = rand::thread_rng();

    //intialize all components, and virtual change emulator proxy
    let mut virtual_change_emulator = VirtualChangeEmulator::new();
    let utxoindex_db = create_temp_db();
    let consensus_db = create_temp_db();
    let test_consensus = Arc::new(TestConsensus::new(consensus_db.1, &DEVNET_PARAMS)); //this functions as a mock, simply to pass onto the utxoindex.
    let utxoindex = UtxoIndex::new(test_consensus.clone(), utxoindex_db.1, virtual_change_emulator.receiver.clone());
    println!("intialized");

    //fill intial utxo collectection in emulator
    virtual_change_emulator.fill_utxo_collection(2000);
    println!("filled utxocollection");


    //change virtual state to get a virtual state in emulator
    virtual_change_emulator.change_virtual_state(200, 200, rng.gen_range(1..3));
    println!("created first state");


    //create a virtual state for test consensus from emulator variables
    let mut test_consensus_virtual_state = VirtualState::default();
    test_consensus_virtual_state.daa_score = virtual_change_emulator.virtual_state.virtual_daa_score;
    test_consensus_virtual_state.parents = virtual_change_emulator.virtual_state.virtual_parents.clone();
    test_consensus_virtual_state.utxo_diff = virtual_change_emulator.virtual_state.virtual_utxo_diff.clone();
    test_consensus_virtual_state.ghostdag_data.blue_score = virtual_change_emulator.virtual_state.virtual_selected_parent_blue_score;
    println!("created virtual state for consensus");


    //write virtual state from emulator to test_consensus db.
    let test_consensus_virtual_store = test_consensus.consensus.virtual_processor.virtual_stores.write();
    let _ = test_consensus_virtual_store.utxo_set.clone().write_diff(&test_consensus_virtual_state.utxo_diff.clone()).expect("expected write diff");
    let _ = test_consensus_virtual_store.state.clone().set(test_consensus_virtual_state).expect("setting of state");
    println!("updated consensus virtual state stores");

    //sync index from scratch and listen to virtual changes. 
    let runner  = utxoindex.clone();
    let _jh = tokio::spawn( async move { runner.run().await } );
    println!("started utxoindex");

    //test sync from scratch.
    let consensus_utxos = test_consensus.clone().get_virtual_utxos(None, usize::MAX); // `usize::MAX` to ensure to get all.   
    println!("syncing");
    let mut i = 0;
    for (tx_outpoint, utxo_entry) in consensus_utxos.iter() {
        let indexed_utxos = utxoindex
            .get_utxos_by_script_public_keys(HashSet::from_iter(vec![utxo_entry.script_public_key.clone()]))
            .expect("expected script public key to be in database");
        for (indexed_script_public_key, indexed_compact_utxo_collection) in indexed_utxos.iter() {
            assert_eq!(*indexed_script_public_key, utxo_entry.script_public_key);
            for (indexed_tx_outpoint, indexed_compact_utxo) in indexed_compact_utxo_collection.iter() {
                assert_eq!(*indexed_tx_outpoint, *tx_outpoint);
                assert_eq!(utxo_entry.amount, indexed_compact_utxo.amount);
                assert_eq!(utxo_entry.block_daa_score, indexed_compact_utxo.block_daa_score);
                assert_eq!(utxo_entry.is_coinbase, indexed_compact_utxo.is_coinbase);
                i += 1;
            }
        }
    }
    assert_eq!(i, consensus_utxos.len());
    println!("passed sync from scratch test");

    //test update 
    virtual_change_emulator.clear_virtual_state();
    virtual_change_emulator.change_virtual_state(rng.gen_range(120..=200), rng.gen_range(120..=200), rng.gen_range(1..3));
    virtual_change_emulator.signal_intial_state();
    println!("updated virtual state");
    
    let res = utxoindex.rpc_receiver.recv().await.expect("expected notification");
    match res {
        utxoindex::notify::UtxoIndexNotification::UtxosChanged(utxo_changed) => {
            for (tx_outpoint, utxo_entry) in virtual_change_emulator.virtual_state.virtual_utxo_diff.add.iter() {
                let indexed_utxos = utxoindex
                    .get_utxos_by_script_public_keys(HashSet::from_iter(vec![utxo_entry.script_public_key.clone()]))
                    .expect("expected script public key to be in database");
                    let indexed_utxo = indexed_utxos.get(&utxo_entry.script_public_key).expect("expected script public key").get(tx_outpoint).expect("expected transaction outpoint");
                    assert_eq!(utxo_entry.amount, indexed_utxo.amount);
                    assert_eq!(utxo_entry.block_daa_score, indexed_utxo.block_daa_score);
                    assert_eq!(utxo_entry.is_coinbase, indexed_utxo.is_coinbase);
                    }

            for (tx_outpoint, utxo_entry) in virtual_change_emulator.virtual_state.virtual_utxo_diff.remove.iter() {
                match utxoindex
                    .get_utxos_by_script_public_keys(HashSet::from_iter(vec![utxo_entry.script_public_key.clone()])) {
                        Ok(indexed_collection) => {
                            if indexed_collection.contains_key(&utxo_entry.script_public_key) {
                                assert!(!indexed_collection.get(&utxo_entry.script_public_key).expect("expected script public key key").contains_key(tx_outpoint))
                            }
                        }
                        Err(err) => match err {
                            consensus::model::stores::errors::StoreError::KeyNotFound(_) => continue,
                            _ => panic!("could not read from database {}", err)
                        }
                    }
                }
            }
        }
    println!("passed utxo update test");        
    utxoindex.signal_shutdown();
    utxoindex.shutdown_finalized_listener.clone().await;
    println!("shutdown successful");        
    drop(test_consensus_virtual_store);
    drop(virtual_change_emulator);
    drop(utxoindex);
    drop(test_consensus);

}
