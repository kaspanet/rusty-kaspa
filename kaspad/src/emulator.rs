use consensus::{
    consensus::test_consensus::TestConsensus, errors::RuleError, model::stores::statuses::BlockStatus, pipeline::ProcessingCounters,
};
use futures_util::future::join_all;
use hashes::Hash;
use kaspa_core::{core::Core, service::Service, signals::Shutdown, trace};
use num_format::{Locale, ToFormattedString};
use rand_distr::{Distribution, Poisson};
use std::{
    cmp::min,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, spawn, JoinHandle},
    time::{Duration, SystemTime},
};

/// Emits blocks randomly in the round-based model where number of
/// blocks in each round is distributed ~ Poisson(bps * delay).
pub struct RandomBlockEmitter {
    terminate: AtomicBool,
    consensus: Arc<TestConsensus>,
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
        consensus: Arc<TestConsensus>,
        genesis: Hash,
        max_block_parents: u64,
        bps: f64,
        delay: f64,
        target_blocks: u64,
    ) -> Self {
        let counters = consensus.processing_counters().clone();
        Self { terminate: AtomicBool::new(false), consensus, genesis, max_block_parents, bps, delay, target_blocks, counters }
    }

    #[tokio::main]
    pub async fn worker(self: &Arc<RandomBlockEmitter>, core: Arc<Core>) {
        let poi = Poisson::new(self.bps * self.delay).unwrap();
        let mut thread_rng = rand::thread_rng();

        let mut tips = vec![self.genesis];
        let mut total = 0;

        while total < self.target_blocks {
            let v = min(self.max_block_parents, poi.sample(&mut thread_rng) as u64);
            if v == 0 {
                continue;
            }

            let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis() as u64;

            if self.terminate.load(Ordering::SeqCst) {
                break;
            }

            let mut new_tips = Vec::with_capacity(v as usize);
            let mut futures = Vec::new();

            self.counters.blocks_submitted.fetch_add(v, Ordering::SeqCst);

            for i in 0..v {
                // Create a new block referencing all tips from the previous round
                let mut b = self.consensus.build_block_with_parents(Default::default(), tips.clone());
                b.header.timestamp = timestamp;
                b.header.nonce = i;
                b.header.finalize();
                new_tips.push(b.header.hash);
                // Submit to consensus
                let f = self.consensus.validate_and_insert_block(b.to_immutable());
                futures.push(f);
            }
            join_all(futures).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>().unwrap();

            tips = new_tips;
            total += v;
        }
        core.shutdown();
    }
}

impl Service for RandomBlockEmitter {
    fn ident(self: Arc<RandomBlockEmitter>) -> &'static str {
        "block-emitter"
    }

    fn start(self: Arc<RandomBlockEmitter>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![spawn(move || self.worker(core))]
    }

    fn stop(self: Arc<RandomBlockEmitter>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}

impl Shutdown for RandomBlockEmitter {
    fn shutdown(self: &Arc<Self>) {
        self.terminate.store(true, Ordering::SeqCst);
    }
}

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
            let pending = snapshot.blocks_submitted - snapshot.header_counts;

            trace!(
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
