use consensus::consensus::test_consensus::{create_temp_db, TestConsensus};
use consensus_core::utxo::utxo_collection::UtxoCollection;
use utxoindex::utxoindex::UtxoIndex;
use std::sync::Arc;
use rand::Rng;
use consensus::params::DEVNET_PARAMS;
use test_helpers::VirtualChangeEmulator;
use tokio;

#[test]
fn test_utxoindex() {
    let mut rng = rand::thread_rng();
    let virtual_change_emulator = VirtualChangeEmulator::new();
    virtual_change_emulator.fill_utxo_collection(10_000);
    let utxoindex_db = create_temp_db();
    let test_consensus = Arc::new(TestConsensus::create_from_temp_db(&DEVNET_PARAMS)); //this functions as a mock, simply to pass onto the utxoindex.
    let utxoindex = UtxoIndex::new(test_consensus.clone(), utxoindex_db.1, virtual_change_emulator.receiver.clone());
    utxoindex.maybe_reset();
    utxoindex.process_events();
    virtual_change_emulator.change_virtual_state(rng.gen_range(0..200), rng,gen_range(0..200))
}