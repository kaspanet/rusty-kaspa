extern crate consensus;
extern crate core;
extern crate hashes;

use std::sync::Arc;

use consensus::consensus::test_consensus::TestConsensus;
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

    let core = Arc::new(Core::new());

    // ---

    let mut params = MAINNET_PARAMS.clone_with_skip_pow();
    params.genesis_hash = genesis;

    // Make sure to create the DB first, so it cleans up last
    let consensus = Arc::new(TestConsensus::create_from_temp_db(&params));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.processing_counters().clone()));
    let emitter = Arc::new(emulator::RandomBlockEmitter::new(
        consensus.clone(),
        genesis,
        params.max_block_parents.into(),
        bps,
        delay,
        target_blocks,
    ));

    // Bind the keyboard signal to the emitter. The emitter will then shutdown core
    Arc::new(signals::Signals::new(&emitter)).init();

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus);
    core.bind(emitter);
    core.bind(monitor);

    core.run();

    trace!("Kaspad is finished...");
}
