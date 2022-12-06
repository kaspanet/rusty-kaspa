use consensus::pipeline::ProcessingCounters;
use kaspa_core::{core::Core, info, service::Service, trace};
use num_format::{Locale, ToFormattedString};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, spawn, JoinHandle},
    time::Duration,
};

pub struct ConsensusMonitor {
    terminate: AtomicBool,
    // Counters
    counters: Arc<ProcessingCounters>,
}

impl ConsensusMonitor {
    pub fn new(counters: Arc<ProcessingCounters>) -> ConsensusMonitor {
        ConsensusMonitor { terminate: AtomicBool::new(false), counters }
    }

    pub fn worker(self: &Arc<ConsensusMonitor>) {
        let mut last_snapshot = self.counters.snapshot();

        loop {
            thread::sleep(Duration::from_millis(1000));

            if self.terminate.load(Ordering::SeqCst) {
                break;
            }

            let snapshot = self.counters.snapshot();

            let send_rate = snapshot.blocks_submitted - last_snapshot.blocks_submitted;
            let header_rate = snapshot.header_counts - last_snapshot.header_counts;
            let deps_rate = snapshot.dep_counts - last_snapshot.dep_counts;
            let pending: i64 = i64::try_from(snapshot.blocks_submitted).unwrap() - i64::try_from(snapshot.header_counts).unwrap();

            info!(
                "sent: {}, processed: {}, pending: {}, -> send rate b/s: {}, process rate b/s: {}, deps rate e/s: {}",
                snapshot.blocks_submitted.to_formatted_string(&Locale::en),
                snapshot.header_counts.to_formatted_string(&Locale::en),
                pending.to_formatted_string(&Locale::en),
                send_rate.to_formatted_string(&Locale::en),
                header_rate.to_formatted_string(&Locale::en),
                deps_rate.to_formatted_string(&Locale::en),
            );

            last_snapshot = snapshot;
        }

        trace!("monitor thread exiting");
    }
}

// service trait implementation for Monitor
impl Service for ConsensusMonitor {
    fn ident(self: Arc<ConsensusMonitor>) -> &'static str {
        "consensus-monitor"
    }

    fn start(self: Arc<ConsensusMonitor>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker())]
    }

    fn stop(self: Arc<ConsensusMonitor>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}
