extern crate consensus;
extern crate core;
extern crate hashes;

use std::sync::Arc;

use consensus::consensus::test_consensus::create_temp_db;
use consensus::consensus::Consensus;
use consensus::params::MAINNET_PARAMS;
use consensus_core::blockhash;
use hashes::Hash;
use kaspa_core::core::Core;
use kaspa_core::*;

use crate::emulator::ConsensusMonitor;

mod emulator;

pub fn main() {
    let genesis: Hash = blockhash::new_unique();
    let bps = 8.0;
    let delay = 2.0;
    let target_blocks = 32000;

    trace!("Kaspad starting... (round-based simulation with BPS={} and D={})", bps, delay);

    // rayon::ThreadPoolBuilder::new()
    //     .num_threads(8)
    //     .build_global()
    //     .unwrap();

    println!("Using rayon thread pool with {} threads", rayon::current_num_threads());

    // Make sure to create the DB first, so it cleans up last
    let (_temp_db_lifetime, db) = create_temp_db();

    let core = Arc::new(Core::new());
    let signals = Arc::new(signals::Signals::new(&core));
    signals.init();

    // ---

    let mut params = MAINNET_PARAMS;
    params.genesis_hash = genesis;

    let consensus = Arc::new(Consensus::new(db, &params));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.clone()));
    let emitter = Arc::new(emulator::RandomBlockEmitter::new(
        "block-emitter",
        consensus.clone(),
        genesis,
        params.max_block_parents.into(),
        bps,
        delay,
        target_blocks,
    ));

    // we are starting emitter first - channels will buffer
    // until consumers start, however, when shutting down
    // the shutdown will be done in the startup order, resulting
    // in emitter going down first...
    core.bind(emitter);
    core.bind(consensus);
    core.bind(monitor);

    core.run();

    trace!("Kaspad is finished...");
}
