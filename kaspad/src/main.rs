extern crate consensus;
extern crate core;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use kaspa_core::core::Core;
use kaspa_core::*;

mod domain;

const SERVICE_THREADS: usize = 1;
// if sleep time is < 0, sleep is skipped
const EMITTER_SLEEP_TIME_MSEC: i64 = -1;
// const EMITTER_SLEEP_TIME_MSEC : i64 = 1;

pub fn main() {
    trace!("Kaspad starting...");

    let core = Arc::new(Core::new());
    let signals = Arc::new(signals::Signals::new(core.clone()));
    signals.init();

    // ---

    // global atomics tracking messages
    let send_count = Arc::new(AtomicU64::new(0));
    let recv_count = Arc::new(AtomicU64::new(0));

    // monitor thread dumping message counters
    let monitor = Arc::new(monitor::Monitor::new(send_count.clone(), recv_count.clone()));

    let consumer = Arc::new(test_consumer::TestConsumer::new("consumer", recv_count));
    let service = Arc::new(test_service::TestService::new("service", SERVICE_THREADS, consumer.sender().clone()));
    let emitter = Arc::new(test_emitter::TestEmitter::new(
        "emitter",
        EMITTER_SLEEP_TIME_MSEC,
        service.sender().clone(),
        send_count,
    ));

    // signals.bind(&core);
    core.bind(monitor);

    // we are starting emitter first - channels will buffer
    // until consumers start, however, when shutting down
    // the shutdown will be done in the startup order, resulting
    // in emitter going down first...
    core.bind(emitter);
    core.bind(service);
    core.bind(consumer);

    core.run();

    trace!("Kaspad is finished...");
}
