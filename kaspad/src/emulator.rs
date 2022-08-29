use consensus::{consensus::Consensus, constants::BLOCK_VERSION, errors::RuleError, pipeline::ProcessingCounters};
use consensus_core::block::Block;
use futures::future::join_all;
use hashes::Hash;
use kaspa_core::{core::Core, service::Service, trace};
use num_format::{Locale, ToFormattedString};
use rand_distr::{Distribution, Poisson};
use std::{
    cmp::min,
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
    max_block_parents: u64,
    bps: f64,
    delay: f64,
    target_blocks: u64,

    // Counters
    counters: Arc<ProcessingCounters>,
}

impl RandomBlockEmitter {
    pub fn new(
        name: &str, consensus: Arc<Consensus>, genesis: Hash, max_block_parents: u64, bps: f64, delay: f64,
        target_blocks: u64,
    ) -> Self {
        let counters = consensus.counters.clone();
        Self {
            terminate: AtomicBool::new(false),
            name: name.to_string(),
            consensus,
            genesis,
            max_block_parents,
            bps,
            delay,
            target_blocks,
            counters,
        }
    }

    #[tokio::main]
    pub async fn worker(self: &Arc<RandomBlockEmitter>, core: Arc<Core>) {
        let poi = Poisson::new(self.bps * self.delay).unwrap();
        let mut thread_rng = rand::thread_rng();

        let mut tips = vec![self.genesis];
        let mut total = 0;
        let mut timestamp = 0u64;

        while total < self.target_blocks {
            let v = min(self.max_block_parents, poi.sample(&mut thread_rng) as u64);
            timestamp += (self.delay as u64) * 1000;
            if v == 0 {
                continue;
            }

            if self.terminate.load(Ordering::SeqCst) {
                break;
            }

            let mut new_tips = Vec::with_capacity(v as usize);
            let mut futures = Vec::new();

            self.counters
                .blocks_submitted
                .fetch_add(v, Ordering::SeqCst);
                
            for i in 0..v {
                // Create a new block referencing all tips from the previous round
                let b = Block::new(BLOCK_VERSION, tips.clone(), timestamp, 0, i, total);
                new_tips.push(b.header.hash);
                // Submit to consensus
                let f = self
                    .consensus
                    .validate_and_insert_block(Arc::new(b));
                futures.push(f);
            }
            join_all(futures)
                .await
                .into_iter()
                .collect::<Result<Vec<()>, RuleError>>()
                .unwrap();

            tips = new_tips;
            total += v;
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
