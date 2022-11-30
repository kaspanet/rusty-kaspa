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
use rpc_core::server::collector::ConsensusNotificationChannel;
use rpc_core::server::service::RpcCoreService;
use rpc_grpc::server::GrpcServer;

use crate::async_runtime::AsyncRuntime;
use crate::emulator::ConsensusMonitor;

mod async_runtime;
mod emulator;

pub fn main() {
    let genesis: Hash = blockhash::new_unique();
    let bps = 8.0;
    let delay = 2.0;
    let target_blocks = 32000;

    trace!("Kaspad starting... (round-based simulation with BPS={} and D={})", bps, delay);
    trace!("\n\n ------ NOTE: this code is just a placeholder for the actual kaspad code, for an actual simulation run the simpa binary ------\n\n");

    // rayon::ThreadPoolBuilder::new()
    //     .num_threads(8)
    //     .build_global()
    //     .unwrap();

    println!("Using rayon thread pool with {} threads", rayon::current_num_threads());

    let core = Arc::new(Core::new());

    // ---

    let mut params = MAINNET_PARAMS.clone_with_skip_pow();
    params.genesis_hash = genesis;
    params.genesis_timestamp = 0;

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

    let notification_channel = ConsensusNotificationChannel::default();
    let rpc_core_service = Arc::new(RpcCoreService::new(consensus.clone(), notification_channel.receiver()));
    let grpc_server_addr = "[::1]:10000".parse().unwrap();
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_service));
    let async_runtime = Arc::new(AsyncRuntime::_new(grpc_server));

    // Bind the keyboard signal to the emitter. The emitter will then shutdown core
    Arc::new(signals::Signals::new(&emitter)).init();

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus);
    core.bind(emitter);
    core.bind(monitor);
    core.bind(async_runtime);

    core.run();

    trace!("Kaspad is finished...");
}
