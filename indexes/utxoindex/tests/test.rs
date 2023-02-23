use consensus::{
    config::Config,
    consensus::test_consensus::{create_temp_db, TestConsensus},
    model::stores::{
        utxo_set::UtxoSetStore,
        virtual_state::{VirtualState, VirtualStateStore},
    },
    params::DEVNET_PARAMS,
};

use std::{collections::HashSet, sync::Arc, time::Instant};

use consensus_core::{
    api::ConsensusApi,
    utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff},
    BlockHashSet,
};

use kaspa_core::info;
use utxoindex::{api::UtxoIndexApi, model::CirculatingSupply, UtxoIndex};

mod testutils;
use testutils::virtual_change_emulator::VirtualChangeEmulator;

/// TODO: use proper Simnet when implemented.
#[test]
fn test_utxoindex() {
    kaspa_core::log::try_init_logger("INFO");

    let resync_utxo_collection_size = 10_000;
    let update_utxo_collection_size = 1_000;
    let script_public_key_pool_size = 200;

    // Initialize all components, and virtual change emulator proxy.
    let mut virtual_change_emulator = VirtualChangeEmulator::new();
    let utxoindex_db = create_temp_db();
    let test_consensus = Arc::new(TestConsensus::create_from_temp_db_and_dummy_sender(&Config::new(DEVNET_PARAMS)));
    let utxoindex = UtxoIndex::new(test_consensus.consensus(), utxoindex_db.1).unwrap();

    // Fill initial utxo collection in emulator.
    virtual_change_emulator.fill_utxo_collection(resync_utxo_collection_size, script_public_key_pool_size); //10_000 utxos belonging to 100 script public keys

    // Create a virtual state for the test consensus from emulator variables.
    let test_consensus_virtual_state = VirtualState {
        daa_score: 0,
        parents: Vec::from_iter(virtual_change_emulator.tips.clone()),
        utxo_diff: UtxoDiff::new(virtual_change_emulator.utxo_collection.clone(), UtxoCollection::new()),
        ..Default::default()
    };
    // Write virtual state from emulator to test_consensus db.
    test_consensus
        .consensus
        .virtual_processor
        .virtual_stores
        .write()
        .utxo_set
        .write_diff(&test_consensus_virtual_state.utxo_diff)
        .expect("expected write diff");
    test_consensus
        .consensus
        .virtual_processor
        .virtual_stores
        .write()
        .state
        .set(test_consensus_virtual_state)
        .expect("setting of state");

    // Sync utxoindex from scratch.
    assert!(!utxoindex.read().is_synced().expect("expected bool"));
    let now = Instant::now(); // TODO: move over to proper benching eventually.
    utxoindex.write().resync().expect("expected resync");
    let bench_time = now.elapsed().as_millis();
    info!(
        "re-synced {0} utxos from {1} script public keys in {2} ms, (note: run test with `--release` for accurate results)",
        resync_utxo_collection_size, script_public_key_pool_size, bench_time
    ); // Ad-hoc benchmark (run with --release)
    assert!(utxoindex.read().is_synced().expect("expected bool"));

    // Test the sync from scratch via consensus db.
    let consensus_utxos = test_consensus.consensus().get_virtual_utxos(None, usize::MAX, false); // `usize::MAX` to ensure to get all.
    let mut i = 0;
    let mut consensus_supply: CirculatingSupply = 0;
    let consensus_utxo_set_size = consensus_utxos.len();
    for (tx_outpoint, utxo_entry) in consensus_utxos.into_iter() {
        consensus_supply += utxo_entry.amount;
        let indexed_utxos = utxoindex
            .read()
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

    assert_eq!(i, consensus_utxo_set_size);

    assert_eq!(utxoindex.read().get_circulating_supply().expect("expected circulating supply"), consensus_supply);
    assert_eq!(
        *utxoindex.read().get_utxo_index_tips().expect("expected circulating supply"),
        BlockHashSet::from_iter(test_consensus.consensus().get_virtual_state_tips())
    );

    // Test update: Change and signal new virtual state.
    virtual_change_emulator.clear_virtual_state();
    virtual_change_emulator.change_virtual_state(update_utxo_collection_size, update_utxo_collection_size, 1);

    let now = Instant::now();
    let utxo_changes = utxoindex
        .write()
        .update(virtual_change_emulator.virtual_state.accumulated_utxo_diff.clone(), virtual_change_emulator.virtual_state.parents)
        .expect("expected utxoindex utxo changes");
    let bench_time = now.elapsed().as_millis();
    // TODO: move over to proper benching eventually.
    info!(
        "updated {0} utxos from {1} script public keys in {2} ms, (note: run test with `--release` for accurate results)",
        update_utxo_collection_size, script_public_key_pool_size, bench_time
    ); //ad-hoc benchmark (run with --release)

    let mut i = 0;
    for (script_public_key, compact_utxo_collection) in utxo_changes.added.iter() {
        for (tx_outpoint, compact_utxo_entry) in compact_utxo_collection.iter() {
            let utxo_entry =
                virtual_change_emulator.virtual_state.accumulated_utxo_diff.add.get(tx_outpoint).expect("expected utxo_entry");
            assert_eq!(*script_public_key, utxo_entry.script_public_key);
            assert_eq!(compact_utxo_entry.amount, utxo_entry.amount);
            assert_eq!(compact_utxo_entry.block_daa_score, utxo_entry.block_daa_score);
            assert_eq!(compact_utxo_entry.is_coinbase, utxo_entry.is_coinbase);
            i += 1;
        }
    }
    assert_eq!(i, virtual_change_emulator.virtual_state.accumulated_utxo_diff.add.len());

    i = 0;

    for (script_public_key, compact_utxo_collection) in utxo_changes.removed.iter() {
        for (tx_outpoint, compact_utxo_entry) in compact_utxo_collection.iter() {
            assert!(virtual_change_emulator.virtual_state.accumulated_utxo_diff.remove.contains_key(tx_outpoint));
            let utxo_entry =
                virtual_change_emulator.virtual_state.accumulated_utxo_diff.remove.get(tx_outpoint).expect("expected utxo_entry");
            assert_eq!(*script_public_key, utxo_entry.script_public_key);
            assert_eq!(compact_utxo_entry.amount, utxo_entry.amount);
            assert_eq!(compact_utxo_entry.block_daa_score, utxo_entry.block_daa_score);
            assert_eq!(compact_utxo_entry.is_coinbase, utxo_entry.is_coinbase);
            i += 1;
        }
    }
    assert_eq!(i, virtual_change_emulator.virtual_state.accumulated_utxo_diff.remove.len());

    assert_eq!(
        utxoindex.read().get_circulating_supply().expect("expected circulating supply"),
        virtual_change_emulator.circulating_supply
    );
    assert_eq!(*utxoindex.read().get_utxo_index_tips().expect("expected circulating supply"), virtual_change_emulator.tips);

    //test if resync clears db.

    utxoindex.write().resync().expect("expected resync");

    // Since we changed virtual state in the emulator, but not in test-consensus db,
    // we expect the resync to get the utxo-set from the test-consensus,
    // these utxos correspond the the initial sync test.
    let consensus_utxos = test_consensus.consensus().get_virtual_utxos(None, usize::MAX, false); // `usize::MAX` to ensure to get all.
    let mut i = 0;
    let consensus_utxo_set_size = consensus_utxos.len();
    for (tx_outpoint, utxo_entry) in consensus_utxos.into_iter() {
        let indexed_utxos = utxoindex
            .read()
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
    assert_eq!(i, consensus_utxo_set_size);

    assert_eq!(
        *utxoindex.read().get_utxo_index_tips().expect("expected circulating supply"),
        BlockHashSet::from_iter(test_consensus.consensus().get_virtual_state_tips())
    );

    // Deconstruct
    drop(utxoindex);
    drop(test_consensus);
}
