use consensus::{consensus::Consensus, pipeline::ProcessingCounters};
use consensus_core::block::Block;
use hashes::Hash;
use kaspa_core::{core::Core, service::Service, trace};
use num_format::{Locale, ToFormattedString};
use rand_distr::{Distribution, Poisson};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, spawn, JoinHandle},
    time::Duration,
};

/// Emits blocks randomly in the round-based model where number of
/// blocks in each round is distributed ~ Poisson(bps * delay).
pub struct RandomBlockEmitter {
    terminate: AtomicBool,
    name: String,
    consensus: Arc<Consensus>,
    genesis: Hash,
    bps: f64,
    delay: f64,
    target_blocks: u64,

    // Counters
    counters: Arc<ProcessingCounters>,
}

impl RandomBlockEmitter {
    pub fn new(name: &str, consensus: Arc<Consensus>, genesis: Hash, bps: f64, delay: f64, target_blocks: u64) -> Self {
        let counters = consensus.counters.clone();
        Self {
            terminate: AtomicBool::new(false),
            name: name.to_string(),
            consensus,
            genesis,
            bps,
            delay,
            target_blocks,
            counters,
        }
    }

    pub fn worker(self: &Arc<RandomBlockEmitter>, core: Arc<Core>) {
        let poi = Poisson::new(self.bps * self.delay).unwrap();
        let mut thread_rng = rand::thread_rng();

        let mut tips = vec![self.genesis];
        let mut total = 0;

        while total < self.target_blocks {
            let v = poi.sample(&mut thread_rng) as u64;
            if v == 0 {
                continue;
            }
            total += v;

            if self.terminate.load(Ordering::SeqCst) {
                break;
            }

            let mut new_tips = Vec::with_capacity(v as usize);
            for i in 0..v {
                // Create a new block referencing all tips from the previous round
                let b = Block::new(0, tips.clone(), i, todo!());
                new_tips.push(b.header.hash);
                // Submit to consensus
                self.consensus
                    .validate_and_insert_block(Arc::new(b));
            }
            tips = new_tips;
            self.counters
                .blocks_submitted
                .fetch_add(v, Ordering::Relaxed);
        }
        self.consensus.signal_exit();
        thread::sleep(Duration::from_millis(4000));
        core.shutdown();
    }
}

impl Service for RandomBlockEmitter {
    fn ident(self: Arc<RandomBlockEmitter>) -> String {
        self.name.clone()
    }

    fn start(self: Arc<RandomBlockEmitter>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self: Arc<RandomBlockEmitter>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}

pub struct ConsensusMonitor {
    terminate: AtomicBool,
    // Counters
    counters: Arc<ProcessingCounters>,
}

impl ConsensusMonitor {
    pub fn new(consensus: Arc<Consensus>) -> ConsensusMonitor {
        ConsensusMonitor { terminate: AtomicBool::new(false), counters: consensus.counters.clone() }
    }

    pub fn worker(self: &Arc<ConsensusMonitor>) {
        let mut last_snapshot = self.counters.snapshot();

        loop {
            thread::sleep(Duration::from_millis(1000));

            let snapshot = self.counters.snapshot();

            let send_rate = snapshot.blocks_submitted - last_snapshot.blocks_submitted;
            let header_rate = snapshot.header_counts - last_snapshot.header_counts;
            let deps_rate = snapshot.dep_counts - last_snapshot.dep_counts;
            let pending = snapshot.blocks_submitted - snapshot.header_counts;

            trace!(
                "sent: {}, processed: {}, pending: {}, -> send rate b/s: {}, process rate b/s: {}, deps rate e/s: {}",
                snapshot
                    .blocks_submitted
                    .to_formatted_string(&Locale::en),
                snapshot
                    .header_counts
                    .to_formatted_string(&Locale::en),
                pending.to_formatted_string(&Locale::en),
                send_rate.to_formatted_string(&Locale::en),
                header_rate.to_formatted_string(&Locale::en),
                deps_rate.to_formatted_string(&Locale::en),
            );

            last_snapshot = snapshot;

            if self.terminate.load(Ordering::SeqCst) {
                break;
            }
        }

        trace!("monitor thread exiting");
    }
}

// service trait implementation for Monitor
impl Service for ConsensusMonitor {
    fn ident(self: Arc<ConsensusMonitor>) -> String {
        "consensus-monitor".into()
    }

    fn start(self: Arc<ConsensusMonitor>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker())]
    }

    fn stop(self: Arc<ConsensusMonitor>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}
