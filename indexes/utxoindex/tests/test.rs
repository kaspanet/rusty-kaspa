use consensus::consensus::test_consensus::{create_temp_db, TestConsensus};
use consensus::model::stores::utxo_set::UtxoSetStore;
use consensus::model::stores::virtual_state::VirtualState;
use consensus::model::stores::virtual_state::VirtualStateStore;
use consensus::params::DEVNET_PARAMS;
use consensus_core::api::ConsensusApi;
use consensus_core::tx::TransactionOutpoint;
use consensus_core::utxo::utxo_collection::UtxoCollection;
use consensus_core::utxo::utxo_diff::UtxoDiff;
use rand::Rng;
use utxoindex::core::indexed_utxos;
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;
use tokio::test;
use utxoindex::core::api::UtxoIndexApi;
use utxoindex::test_helpers::VirtualChangeEmulator;
use utxoindex::{core::api::DynUtxoIndex, core::notify::UtxoIndexNotification, utxoindex::UtxoIndex};

/// This test uses an ad hoc, ineffecient and preliminary testing platform, utilizing a custom virtual change emulator.
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
    virtual_change_emulator.fill_utxo_collection(1_000, 500); //10_000 utxos belonging to 200 script public keys
    println!("filled utxocollection");

    println!("{}", virtual_change_emulator.utxo_collection.len());
    //create a virtual state for test consensus from emulator variables
    let mut test_consensus_virtual_state = VirtualState::default();
    test_consensus_virtual_state.daa_score = 0;
    test_consensus_virtual_state.parents = Vec::from_iter(virtual_change_emulator.tips.clone());
    test_consensus_virtual_state.utxo_diff = UtxoDiff::new(virtual_change_emulator.utxo_collection.clone(), UtxoCollection::new());
    test_consensus_virtual_state.ghostdag_data.blue_score = 0;
    println!("created virtual state for consensus");

    //write virtual state from emulator to test_consensus db.
    let test_consensus_virtual_store = test_consensus.consensus.virtual_processor.virtual_stores.write();
    let _ = test_consensus_virtual_store
        .utxo_set
        .clone()
        .write_diff(&test_consensus_virtual_state.utxo_diff)
        .expect("expected write diff");
    let _ = test_consensus_virtual_store.state.clone().set(test_consensus_virtual_state).expect("setting of state");
    drop(test_consensus_virtual_store);
    println!("updated consensus virtual state stores");

    //sync index from scratch and listen to virtual changes.
    //let jh = tokio::spawn( async move { runner.run().await } );
    utxoindex.maybe_reset().expect("expected reset");
    println!("started utxoindex");

    //test sync from scratch from consensus db. (note: all other tests are done via the emulator)
    let consensus_utxos = test_consensus.clone().get_virtual_utxos(None, usize::MAX); // `usize::MAX` to ensure to get all.
    println!("syncing");
    println!("{}", consensus_utxos.len());
    let mut i = 0;
    let consnesus_utxo_set_size = consensus_utxos.len();
    for (tx_outpoint, utxo_entry) in consensus_utxos.into_iter() {
        let indexed_utxos = utxoindex
            .get_utxos_by_script_public_keys(HashSet::from_iter(vec![utxo_entry.script_public_key.clone()]))
            .expect("expected script public key to be in database");
        for (indexed_script_public_key, indexed_compact_utxo_collection) in indexed_utxos.into_iter() {
            let compact_utxo = indexed_compact_utxo_collection.get(&tx_outpoint).expect("expected outpoint as key");
            assert_eq!(indexed_script_public_key, utxo_entry.script_public_key);
            assert_eq!(utxo_entry.amount, compact_utxo.amount);
            assert_eq!(utxo_entry.block_daa_score, compact_utxo.block_daa_score);
            assert_eq!(utxo_entry.is_coinbase, compact_utxo.is_coinbase);
            i += 1;
        }
    }
    assert_eq!(i, consnesus_utxo_set_size);

    assert_eq!(utxoindex.get_circulating_supply().expect("expected circulating supply"), virtual_change_emulator.circulating_supply);
    assert_eq!(*utxoindex.stores.get_tips().expect("expected circulating supply"), virtual_change_emulator.tips);

    println!("passed sync from scratch test");

    //start utxoindex processor
    let processer = utxoindex.clone();
    tokio::spawn(async move { processer.process_events().await });
    println!("utxoindex listening for events");

    // #test update:
    // ##Change and signal new virtual state.
    virtual_change_emulator.clear_virtual_state();
    virtual_change_emulator.change_virtual_state(rng.gen_range(120..=200), rng.gen_range(120..=200), rng.gen_range(1..3));
    virtual_change_emulator.signal_virtual_state();
    println!("updated virtual state");

    let res = utxoindex.rpc_receiver.recv().await.expect("expected notification");
    match res {
        UtxoIndexNotification::UtxosChanged(utxo_changed) => {
            let mut i = 0;
            for (script_public_key, compact_utxo_collection) in utxo_changed.added.iter() {
                for (tx_outpoint, compact_utxo_entry) in compact_utxo_collection.iter() {
                    let utxo_entry =
                        virtual_change_emulator.virtual_state.virtual_utxo_diff.add.get(tx_outpoint).expect("expected utxo_entry");
                    assert_eq!(*script_public_key, utxo_entry.script_public_key);
                    assert_eq!(compact_utxo_entry.amount, utxo_entry.amount);
                    assert_eq!(compact_utxo_entry.block_daa_score, utxo_entry.block_daa_score);
                    assert_eq!(compact_utxo_entry.is_coinbase, utxo_entry.is_coinbase);
                    i += 1;
                }
            }
            assert_eq!(i, virtual_change_emulator.virtual_state.virtual_utxo_diff.add.len());

            i = 0;

            for (script_public_key, compact_utxo_collection) in utxo_changed.removed.iter() {
                for (tx_outpoint, compact_utxo_entry) in compact_utxo_collection.iter() {
                    assert!(virtual_change_emulator.virtual_state.virtual_utxo_diff.remove.contains_key(tx_outpoint));
                    let utxo_entry =
                        virtual_change_emulator.virtual_state.virtual_utxo_diff.remove.get(tx_outpoint).expect("expected utxo_entry");
                    assert_eq!(*script_public_key, utxo_entry.script_public_key);
                    assert_eq!(compact_utxo_entry.amount, utxo_entry.amount);
                    assert_eq!(compact_utxo_entry.block_daa_score, utxo_entry.block_daa_score);
                    assert_eq!(compact_utxo_entry.is_coinbase, utxo_entry.is_coinbase);
                    i += 1;
                }
            }
            assert_eq!(i, virtual_change_emulator.virtual_state.virtual_utxo_diff.remove.len());
        }
    }

    assert_eq!(utxoindex.get_circulating_supply().expect("expected circulating supply"), virtual_change_emulator.circulating_supply);
    assert_eq!(*utxoindex.stores.get_tips().expect("expected circulating supply"), virtual_change_emulator.tips);

    //test if endstate is same as emulator end state
    let mut i = 0;
    for (script_public_key, compact_utxo_collection) in utxoindex.get_all_utxos().expect("expected utxos") {
        for (tx_outpoint, compact_utxo) in compact_utxo_collection.iter() {
            assert!(virtual_change_emulator.utxo_collection.contains_key(tx_outpoint));
            let utxo_entry = virtual_change_emulator.utxo_collection.get(tx_outpoint).expect("expected outpoint as key");
            assert_eq!(utxo_entry.script_public_key, script_public_key);
            assert_eq!(utxo_entry.amount, compact_utxo.amount);
            assert_eq!(utxo_entry.block_daa_score, compact_utxo.block_daa_score);
            assert_eq!(utxo_entry.is_coinbase, compact_utxo.is_coinbase);
            i += 1;
        }
    }
    assert_eq!(i, virtual_change_emulator.utxo_collection.len());

    assert_eq!(utxoindex.get_circulating_supply().expect("expected circulating supply"), virtual_change_emulator.circulating_supply);
    assert_eq!(*utxoindex.stores.get_tips().expect("expected circulating supply"), virtual_change_emulator.tips);

    //test shut-down
    println!("shutting down");
    utxoindex.signal_shutdown();
    utxoindex.shutdown_finalized_listener.clone().await;
    println!("shutdown successful");

    //deconstuct
    drop(virtual_change_emulator);
    drop(utxoindex);
    drop(test_consensus);
    println!("done");
}
