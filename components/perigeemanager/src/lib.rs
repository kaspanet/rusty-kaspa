use std::{
    cmp::{min, Ordering},
    collections::{hash_map::Entry, HashMap, HashSet},
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use itertools::Itertools;
use kaspa_consensus_core::{BlockHashSet, Hash, HashMapCustomHasher};
use kaspa_core::{info, trace};
use kaspa_p2p_lib::{Hub, PeerKey, Router};
use log::debug;
use parking_lot::Mutex;
use rand::{seq::IteratorRandom, thread_rng, Rng};

// Tolerance for the amount of blocks verified in a round to trigger evaluation.
// For example, if We expect to see 100 blocks verified in a round, but less or more than [`BLOCKS_VERIFIED_TOLERANCE`]
// (i.e., 15 / 85 blocks) are verified, we skip the evaluation for exploitation this round.
// reasoning is that network conditions are then not considered stable enough to make a good decision.
// and we rather skip, and wait for the next round.
// Note that exploration can still happen even if this threshold is not met.
// This ensures that we continue to explore in case network conditions are fault of the peers, and not oneself.
const BLOCKS_VERIFIED_FAULT_TOLERANCE: f64 = 0.175;

pub struct PeerScore {
    p90: u64,
    p95: u64,
    p97_5: u64,
    p98_25: u64,
    p99_125: u64,
    p99_6875: u64,
    p100: u64,
}

impl PeerScore {
    #[inline(always)]
    fn new(p90: u64, p95: u64, p97_5: u64, p98_25: u64, p99_125: u64, p99_6875: u64, p100: u64) -> Self {
        PeerScore { p90, p95, p97_5, p98_25, p99_125, p99_6875, p100 }
    }
}

impl Default for PeerScore {
    #[inline(always)]
    fn default() -> Self {
        Self::new(u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX, u64::MAX)
    }
}

impl PartialEq for PeerScore {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        (self.p90, self.p95, self.p97_5, self.p98_25, self.p99_125, self.p99_6875, self.p100)
            == (other.p90, other.p95, other.p97_5, other.p98_25, other.p99_125, other.p99_6875, other.p100)
    }
}

impl Eq for PeerScore {}

impl PartialOrd for PeerScore {
    #[inline(always)]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PeerScore {
    #[inline(always)]
    fn cmp(&self, other: &Self) -> Ordering {
        (self.p90, self.p95, self.p97_5, self.p98_25, self.p99_125, self.p99_6875, self.p100).cmp(&(
            other.p90,
            other.p95,
            other.p97_5,
            other.p98_25,
            other.p99_125,
            other.p99_6875,
            other.p100,
        ))
    }
}

pub struct PerigeeEvaluationResult {
    pub selected_peers: Vec<PeerKey>,
    pub eviction_peers: Vec<PeerKey>,
    pub skipped_exploitation: bool,
    pub skipped_exploration: bool,
}

#[derive(Debug, Clone)]
pub struct PerigeeConfig {
    pub perigee_outbound_target: usize,
    pub exploitation_target: usize,
    pub exploration_target: usize,
    pub round_frequency: usize,
    pub round_duration_seconds: Duration,
    pub expected_blocks_per_round: usize,
    pub statistics: bool,
    pub persistence: bool,
}

impl PerigeeConfig {
    pub fn new(
        perigee_outbound_target: usize,
        exploitation_target: usize,
        exploration_target: usize,
        round_frequency: usize,
        connection_manager_tick_duration: Duration,
        statistics: bool,
        persistence: bool,
        bps: usize,
    ) -> Self {
        let round_duration_seconds =
            connection_manager_tick_duration.checked_mul(round_frequency as u32).expect("perigee round frequecy overflowed");
        let expected_blocks_per_round = (bps as f64 * round_duration_seconds.as_secs_f64()) as usize;
        Self {
            perigee_outbound_target,
            exploitation_target,
            exploration_target,
            round_frequency,
            round_duration_seconds,
            expected_blocks_per_round,
            statistics,
            persistence,
        }
    }

    pub fn should_initiate_perigee(&self) -> bool {
        (self.perigee_outbound_target > 0 && self.exploration_target > 0 && self.exploitation_target < self.perigee_outbound_target)
            && self.round_frequency > 0
    }
}

#[derive(Debug)]
pub struct PerigeeManager {
    verified_blocks: BlockHashSet, // holds blocks that are consensus verified.
    first_seen: HashMap<Hash, Instant>,
    last_round_exploited_peers: Vec<PeerKey>,
    round_start: Instant,
    round_counter: u64,
    config: PerigeeConfig,
    hub: Hub,
    is_ibd_running: Arc<AtomicBool>,
}

impl PerigeeManager {
    pub fn new(hub: Hub, config: PerigeeConfig, is_ibd_running: Arc<AtomicBool>) -> Mutex<Self> {
        Mutex::new(Self {
            verified_blocks: BlockHashSet::new(),
            first_seen: HashMap::new(),
            last_round_exploited_peers: Vec::new(),
            round_start: Instant::now(),
            round_counter: 0,
            config,
            hub,
            is_ibd_running,
        })
    }

    pub fn insert_perigee_timestamp(&mut self, router: &Arc<Router>, hash: Hash, timestamp: Instant, verify: bool) {
        if router.is_perigee() || (self.config.statistics && router.is_random_graph()) {
            router.add_perigee_timestamp(hash, timestamp);
        }
        if verify {
            self.verify_block(hash);
        }
        self.maybe_insert_first_seen(hash, timestamp);
    }

    pub fn set_initial_persistent_peers(&mut self, peer_keys: Vec<PeerKey>) {
        debug!("PerigeeManager: Setting initial persistent perigee peers for first round");
        self.last_round_exploited_peers = peer_keys
    }

    pub fn is_first_round(&self) -> bool {
        self.round_counter == 0
    }

    pub fn trim_peers(&mut self) -> Vec<PeerKey> {
        debug!("PerigeeManager: Trimming exceess peers from perigee");

        let perigee_routers = self.hub.perigee_routers();
        let to_remove_amount = perigee_routers.len().saturating_sub(self.config.perigee_outbound_target);
        let excused_peer: HashSet<PeerKey> = self.iter_excused_peers(&perigee_routers).into_iter().collect();

        perigee_routers
            .iter()
            .filter(|r| !self.last_round_exploited_peers.contains(&r.key()) && !excused_peer.contains(&r.key()))
            .map(|r| r.key())
            .choose_multiple(&mut thread_rng(), to_remove_amount)
            .iter()
            .chain(excused_peer.iter())
            .take(to_remove_amount)
            .cloned()
            .collect()
    }

    pub fn evaluate_round(&mut self) -> (Vec<PeerKey>, Vec<PeerKey>, bool) {
        trace!("PerigeeManager: evaluating round");
        self.round_counter += 1;

        let (mut peer_table, perigee_routers) = self.build_table();

        let is_ibd_running = self.is_ibd_running();
        self.excuse(&mut peer_table, &perigee_routers);
        let should_exploit = self.should_exploit(is_ibd_running, peer_table.len());
        let should_explore = self.should_explore(is_ibd_running, peer_table.len());
        let mut has_exploited_changed = false;

        if !should_exploit && !should_explore {
            trace!("PerigeeManager: skipping exploitation and exploration this round");
            return (self.last_round_exploited_peers.clone(), vec![], has_exploited_changed);
        }

        // i.e. the peers that we mark as "to exploit" this round.
        let selected_peers = if should_exploit {
            let selected_peers = self.exploit(&mut peer_table);
            debug!(
                "PerigeeManager: Selected peers for exploitation this round: {:?}",
                selected_peers.iter().map(|pk| pk.to_string()).collect_vec()
            );
            if self.last_round_exploited_peers != selected_peers {
                trace!("PerigeeManager: Exploited peers have changed this round");
                has_exploited_changed = true;
                self.last_round_exploited_peers = selected_peers.clone();
            }
            selected_peers
        } else {
            self.last_round_exploited_peers.iter().filter_map(|pk| peer_table.remove_entry(pk).map(|(k, _)| k)).collect()
        };

        // i.e. the peers that we mark as "to evict" this round.
        let deselected_peers = if should_explore { self.explore(&mut peer_table) } else { vec![] };

        (selected_peers, deselected_peers, has_exploited_changed)
    }

    fn exploit(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>) -> Vec<PeerKey> {
        let remaining_table = peer_table;
        let mut selected_table = HashMap::new();
        let mut selected_peers = Vec::new(); // We use this instead of selected_table, to maintain ordering.
        let mut last_score = PeerScore::default();
        for _ in 0..self.config.exploitation_target {
            let (top_ranked, top_ranked_score) = match self.get_top_ranked_peer(remaining_table) {
                (Some(peer), score) => (peer, score),
                _ => break,
            };

            if top_ranked_score == last_score {
                break;
            }

            selected_table.insert(top_ranked, remaining_table.remove(&top_ranked).unwrap());
            selected_peers.push(top_ranked);

            if selected_peers.len() == self.config.exploitation_target {
                break;
            }

            self.transform_peer_table(&mut selected_table, remaining_table);

            last_score = top_ranked_score;
        }
        selected_peers
    }

    fn excuse(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>, perigee_routers: &[Arc<Router>]) {
        for k in self.iter_excused_peers(perigee_routers) {
            peer_table.remove(&k);
        }
    }

    fn explore(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>) -> Vec<PeerKey> {
        let to_remove_target = min(self.config.exploration_target, peer_table.len());

        peer_table.keys().choose_multiple(&mut thread_rng(), to_remove_target).into_iter().cloned().collect()
    }

    pub fn start_new_round(&mut self) {
        self.clear();
        self.round_start = Instant::now();
    }

    pub fn config(&self) -> PerigeeConfig {
        self.config.clone()
    }

    fn maybe_insert_first_seen(&mut self, hash: Hash, timestamp: Instant) {
        match self.first_seen.entry(hash) {
            Entry::Occupied(mut o) => {
                if timestamp < *o.get() {
                    *o.get_mut() = timestamp;
                }
            }
            Entry::Vacant(v) => {
                v.insert(timestamp);
            }
        }
    }

    fn verify_block(&mut self, hash: Hash) {
        self.verified_blocks.insert(hash);
    }

    fn clear(&mut self) {
        debug!["PerigeeManager: Clearing state for new round"];
        self.verified_blocks.clear();
        self.first_seen.clear();
        if self.config.statistics {
            for router in self.hub.random_graph_routers() {
                router.clear_perigee_timestamps();
            }
        };
        for router in self.hub.perigee_routers() {
            router.clear_perigee_timestamps();
        }
    }

    fn iter_excused_peers(&self, perigee_routers: &[Arc<Router>]) -> Vec<PeerKey> {
        perigee_routers.iter().filter(|r| r.connection_started() > self.round_start).map(|r| r.key()).collect()
    }

    fn rate_peer(&self, values: &[u64]) -> PeerScore {
        if values.is_empty() {
            return PeerScore::default();
        }

        let sorted_values = {
            let mut sv = values.to_owned();
            sv.sort_unstable();
            sv
        };

        let len = sorted_values.len();

        // This is defined as the scoring mechanism in the corresponding original perigee paper.
        // it preferences good connectivity to the bulk of the network, while still considering the tail-end delays.
        let p90 = sorted_values[((0.90 * len as f64) as usize).min(len - 1)];

        // This is a deviation from the paper;
        // We rate beyond the p90 to tie-break
        // Testing has exposed that the full Coverage of the p90 range often times typically only requires ~4-6 perigee peers
        // This leaves remaining perigee peers without contribution to latency reduction.
        // as such we rate these even deeper into the tail-end delays to try and increase coverage of outlier blocks.
        let p95 = sorted_values[((0.95 * len as f64) as usize).min(len - 1)];
        let p97_5 = sorted_values[((0.975 * len as f64) as usize).min(len - 1)];
        let p98_25 = sorted_values[((0.9825 * len as f64) as usize).min(len - 1)];
        let p99_125 = sorted_values[((0.99125 * len as f64) as usize).min(len - 1)];
        let p99_6875 = sorted_values[((0.996875 * len as f64) as usize).min(len - 1)];
        let p100 = sorted_values[len - 1];

        PeerScore::new(p90, p95, p97_5, p98_25, p99_125, p99_6875, p100)
    }

    fn get_top_ranked_peer(&self, peer_table: &HashMap<PeerKey, Vec<u64>>) -> (Option<PeerKey>, PeerScore) {
        let mut best_peer: Option<PeerKey> = None;
        let mut best_score = PeerScore::default();
        let mut tied_count = 0;

        for (peer, delays) in peer_table.iter() {
            let score = self.rate_peer(delays);
            if score < best_score {
                best_score = score;
                best_peer = Some(*peer);
            } else if score == best_score {
                tied_count += 1;
                // Randomly replace with probability 1/tied_count
                // this is so that we ensure we don't choose peers based on iteration / Hashmap order
                if thread_rng().gen_ratio(1, tied_count) {
                    best_peer = Some(*peer);
                }
            }
        }

        debug!(
            "PerigeeManager: Top ranked peer from current peer table is {:?} with score p90: {}, p95: {}, p97.5: {}, p98.25: {}, p99.125: {}, p99.6875: {}, p.100: {}",
            best_peer,
            best_score.p90,
            best_score.p95,
            best_score.p97_5,
            best_score.p98_25,
            best_score.p99_125,
            best_score.p99_6875,
            best_score.p100,
        );
        (best_peer, best_score)
    }

    fn transform_peer_table(&self, selected_peers: &mut HashMap<PeerKey, Vec<u64>>, candidates: &mut HashMap<PeerKey, Vec<u64>>) {
        debug!("PerigeeManager: Transforming peer table");

        // sanity check
        // assert all vecs are of equal length
        assert_eq!(
            candidates.values().map(|v| v.len()).chain(selected_peers.values().map(|v| v.len())).all_equal_value().unwrap(),
            self.verified_blocks.len()
        );

        for j in 0..self.verified_blocks.len() {
            let selected_min_j = selected_peers.values().map(|vec| vec[j]).min().unwrap();
            for candidate in candidates.values_mut() {
                // we transform the delay of candidate at pos j to min(candidate_delay_score[j], min(selected_peers_delay_score_at_pos[j])).
                candidate[j] = min(candidate[j], selected_min_j);
            }
        }
    }

    fn should_exploit(&self, is_ibd_running: bool, amount_of_perigee_peer: usize) -> bool {
        // Conditions that need to be met to trigger exploitation:

        // 1. IBD is not running
        !is_ibd_running &&
        // 2. Sufficient blocks have been verified this round
        self.block_threshold_reached() &&
        // 3. We have enough perigee peers to choose from
        amount_of_perigee_peer > self.config.exploitation_target
    }

    fn should_explore(&self, is_ibd_running: bool, amount_of_perigee_peer: usize) -> bool {
        // Conditions that should trigger exploration:

        // 1. IBD is not running
        !is_ibd_running &&
        // 2. We have at least one perigee peer to choose from
        amount_of_perigee_peer.saturating_sub(self.config.exploitation_target) > 0
    }

    fn block_threshold_reached(&self) -> bool {
        let verified_count = self.verified_blocks.len();
        let expected_count = self.config.expected_blocks_per_round;
        let lower_bound = (expected_count as f64 * (1.0 - BLOCKS_VERIFIED_FAULT_TOLERANCE)) as usize;
        let upper_bound = (expected_count as f64 * (1.0 + BLOCKS_VERIFIED_FAULT_TOLERANCE)) as usize;
        verified_count >= lower_bound && verified_count <= upper_bound
    }

    fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn iterate_verified_first_seen(&self) -> impl Iterator<Item = (&Hash, &Instant)> {
        self.first_seen.iter().filter(move |(hash, _)| self.verified_blocks.contains(hash))
    }

    fn build_table(&self) -> (HashMap<PeerKey, Vec<u64>>, Vec<Arc<Router>>) {
        debug!("PerigeeManager: Building peer table");

        let mut peer_table: HashMap<PeerKey, Vec<u64>> = HashMap::new();
        let perigee_routers = self.hub.perigee_routers();

        // below is important as we clone out the hashmap, this should only be done once per round.
        // calling .perigee_timestamps() method in the loop would become expensive.
        let mut perigee_timestamps =
            perigee_routers.iter().map(|r| (r.key(), r.perigee_timestamps())).collect::<HashMap<PeerKey, HashMap<Hash, Instant>>>();

        for (hash, first_ts) in self.iterate_verified_first_seen() {
            for (peer_key, peer_timestamps) in perigee_timestamps.iter_mut() {
                match peer_timestamps.entry(*hash) {
                    Entry::Occupied(o) => {
                        let delay = o.get().duration_since(*first_ts).as_millis() as u64;
                        peer_table.entry(*peer_key).or_default().push(delay);
                    }
                    Entry::Vacant(_) => {
                        peer_table.entry(*peer_key).or_default().push(u64::MAX);
                    }
                }
            }
        }
        (peer_table, perigee_routers)
    }

    pub fn log_statistics(&self) {
        struct DelayStats {
            count: usize,
            mean: f64,
            median: u64,
            min: u64,
            max: u64,
            p90: u64,
            p95: u64,
            p99: u64,
        }

        fn calculate_delay_stats(delays: &[u64]) -> DelayStats {
            if delays.is_empty() {
                return DelayStats { count: 0, mean: 0.0, median: 0, min: 0, max: 0, p90: 0, p95: 0, p99: 0 };
            }

            let sorted = {
                let mut s = delays.to_vec();
                s.sort_unstable();
                s
            };

            let count = sorted.len();
            let mean = sorted.iter().sum::<u64>() as f64 / count as f64;
            let median = sorted[count / 2];
            let min = sorted[0];
            let max = sorted[count - 1];
            let p90 = sorted[((count as f64 * 0.90) as usize).min(count - 1)];
            let p95 = sorted[((count as f64 * 0.95) as usize).min(count - 1)];
            let p99 = sorted[((count as f64 * 0.99) as usize).min(count - 1)];

            DelayStats { count, mean, median, min, max, p90, p95, p99 }
        }

        fn percentage(part: usize, total: usize) -> f64 {
            if total == 0 {
                0.0
            } else {
                (part as f64 / total as f64) * 100.0
            }
        }

        fn improvement_percentage(perigee: f64, random_graph: f64) -> f64 {
            if random_graph == 0.0 {
                0.0
            } else {
                ((random_graph - perigee) / random_graph) * 100.0
            }
        }

        let mut number_of_perigee_peers = 0;
        let perigee_timestamps = self
            .hub
            .perigee_routers()
            .iter()
            .map(|r| {
                number_of_perigee_peers += 1;
                (r.key(), r.perigee_timestamps())
            })
            .collect::<Vec<_>>();
        let mut number_of_random_graph_peers = 0;
        let random_graph_timestamps = self
            .hub
            .random_graph_routers()
            .iter()
            .map(|r| {
                number_of_random_graph_peers += 1;
                (r.key(), r.perigee_timestamps())
            })
            .collect::<Vec<_>>();

        let mut perigee_delays = Vec::new();
        let mut random_graph_delays = Vec::new();
        let mut perigee_wins = 0;
        let mut random_graph_wins = 0;
        let mut ties = 0;

        for (hash, timestamp) in self.iterate_verified_first_seen() {
            let perigee_delay = perigee_timestamps
                .iter()
                .filter_map(|(_, hm)| hm.get(hash).map(|ts| ts.duration_since(*timestamp).as_millis() as u64))
                .min();
            let rg_delay = random_graph_timestamps
                .iter()
                .filter_map(|(_, hm)| hm.get(hash).map(|ts| ts.duration_since(*timestamp).as_millis() as u64))
                .min();

            match (perigee_delay, rg_delay) {
                (Some(p_delay), Some(rg_delay)) => {
                    perigee_delays.push(p_delay);
                    random_graph_delays.push(rg_delay);

                    if p_delay < rg_delay {
                        perigee_wins += 1;
                    } else if rg_delay < p_delay {
                        random_graph_wins += 1;
                    } else {
                        ties += 1;
                    }
                }
                (Some(p_delay), None) => {
                    perigee_delays.push(p_delay);
                    perigee_wins += 1;
                }
                (None, Some(rg_delay)) => {
                    random_graph_delays.push(rg_delay);
                    random_graph_wins += 1;
                }
                (None, None) => {}
            }
        }

        if perigee_delays.is_empty() && random_graph_delays.is_empty() {
            info!("PerigeeManager Statistics: No data available for this round");
            return;
        }

        let perigee_stats = calculate_delay_stats(&perigee_delays);
        let rg_stats = calculate_delay_stats(&random_graph_delays);

        // Log comprehensive statistics
        info!(
            "\n\
     ════════════════════════════════════════════════════════════════════════════ \n\
                           PERIGEE STATISTICS - Round {:4}                     \n\
     ════════════════════════════════════════════════════════════════════════════ \n\
      Config: Out={:<2} Exploit={:<2} Explore={:<2} Duration={:<5}s                   \n\
      Peers:  Perigee={:<2} ({:<5} blks) | Random={:<2} ({:<5} blks)                 \n\
      Blocks: Verified={:<5} | Seen={:<5}                                           \n\
     ════════════════════════════════════════════════════════════════════════════ \n\
      BLOCK DELIVERY RACE                                                         \n\
        Perigee Wins:       {:5} ({:5.1}%)                                         \n\
        Random Graph Wins:  {:5} ({:5.1}%)                                         \n\
        Ties:               {:5} ({:5.1}%)                                         \n\
     ════════════════════════════════════════════════════════════════════════════ \n\
      DELAY STATISTICS (ms)        │  Perigee  │ Random Graph │ Improvement      \n\
     ─────────────────────────────┼───────────┼──────────────┼────────────────── \n\
      Count                        │ {:9} │ {:12} │                 \n\
      Mean                         │ {:9.2} │ {:12.2} │ {:7.2} ({:5.1}%) \n\
      Median                       │ {:9} │ {:12} │ {:7} ({:5.1}%) \n\
      Min                          │ {:9} │ {:12} │                 \n\
      Max                          │ {:9} │ {:12} │                 \n\
      P90                          │ {:9} │ {:12} │ {:7} ({:5.1}%) \n\
      P95                          │ {:9} │ {:12} │                 \n\
      P99                          │ {:9} │ {:12} │                 \n\
     ════════════════════════════════════════════════════════════════════════════ ",
            self.round_counter,
            self.config.perigee_outbound_target,
            self.config.exploitation_target,
            self.config.exploration_target,
            self.config.round_frequency * 30,
            number_of_perigee_peers,
            perigee_timestamps.iter().map(|(_, hm)| hm.len()).sum::<usize>(),
            number_of_random_graph_peers,
            random_graph_timestamps.iter().map(|(_, hm)| hm.len()).sum::<usize>(),
            self.verified_blocks.len(),
            self.first_seen.len(),
            perigee_wins,
            percentage(perigee_wins, perigee_wins + random_graph_wins + ties),
            random_graph_wins,
            percentage(random_graph_wins, perigee_wins + random_graph_wins + ties),
            ties,
            percentage(ties, perigee_wins + random_graph_wins + ties),
            perigee_stats.count,
            rg_stats.count,
            perigee_stats.mean,
            rg_stats.mean,
            rg_stats.mean - perigee_stats.mean,
            improvement_percentage(perigee_stats.mean, rg_stats.mean),
            perigee_stats.median,
            rg_stats.median,
            rg_stats.median as i64 - perigee_stats.median as i64,
            improvement_percentage(perigee_stats.median as f64, rg_stats.median as f64),
            perigee_stats.min,
            rg_stats.min,
            perigee_stats.max,
            rg_stats.max,
            perigee_stats.p90,
            rg_stats.p90,
            rg_stats.p90 as i64 - perigee_stats.p90 as i64,
            improvement_percentage(perigee_stats.p90 as f64, rg_stats.p90 as f64),
            perigee_stats.p95,
            rg_stats.p95,
            perigee_stats.p99,
            rg_stats.p99
        );
    }
}
