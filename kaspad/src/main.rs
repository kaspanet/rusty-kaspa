extern crate consensus;
extern crate core;
extern crate hashes;

use std::sync::Arc;

use consensus::consensus::Consensus;
use consensus::model::stores::ghostdag::KType;
use consensus::model::stores::DB;
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
    let signals = Arc::new(signals::Signals::new(core.clone()));
    signals.init();

    // ---

    let db_tempdir = tempfile::tempdir().unwrap();
    let db = Arc::new(DB::open_default(db_tempdir.path().to_owned().to_str().unwrap()).unwrap());

    let consensus = Arc::new(Consensus::new(db, MAINNET_PARAMS));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.clone()));
    let emitter = Arc::new(emulator::RandomBlockEmitter::new(
        "block-emitter",
        consensus.clone(),
        genesis,
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
