use std::{
    cmp::min,
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::Instant,
};

use itertools::Itertools;
use kaspa_consensus_core::{BlockHashSet, Hash, HashMapCustomHasher};
use kaspa_core::info;
use kaspa_p2p_lib::{Hub, PeerKey, Router};
use log::debug;
use parking_lot::Mutex;
use rand::seq::SliceRandom;

const PERCENTILE_RANK: f64 = 0.9;

#[derive(Debug, Clone)]
pub struct PerigeeConfig {
    pub perigee_outbound_target: usize,
    pub exploitation_target: usize,
    pub exploration_target: usize,
    pub round_frequency: usize,
    pub statistics: bool,
}

impl PerigeeConfig {
    pub fn new(
        perigee_outbound_target: usize,
        exploitation_target: usize,
        exploration_target: usize,
        round_frequency: usize,
        statistics: bool,
    ) -> Self {
        Self { perigee_outbound_target, exploitation_target, exploration_target, round_frequency, statistics }
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
    round_start: Instant,
    round_counter: u64,
    config: PerigeeConfig,
    hub: Hub,
}

impl PerigeeManager {
    pub fn new(hub: Hub, config: PerigeeConfig) -> Mutex<Self> {
        Mutex::new(Self {
            verified_blocks: BlockHashSet::new(),
            first_seen: HashMap::new(),
            round_start: Instant::now(),
            round_counter: 0,
            config,
            hub,
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

    pub fn evaluate_round(&mut self, trim_excess_only: bool) -> (Vec<PeerKey>, Vec<PeerKey>) {
        debug!("PerigeeManager: evaluating round");
        self.round_counter += 1;

        let (peer_table, perigee_routers) = self.build_table();

        let active_perigee_amount = perigee_routers.len();

        if active_perigee_amount <= self.config.exploitation_target {
            debug!("PerigeeManager: not enough peers for round");
            //need to wait for more peers
            return (peer_table.keys().cloned().collect(), vec![]);
        };

        assert!(active_perigee_amount == peer_table.len());

        let mut remaining_table = peer_table;
        let mut selected_table = HashMap::new();
        let mut selected_peers = Vec::new(); // We use this instead of selected_table, to maintain ordering.

        for _ in 0..self.config.exploitation_target {
            let top_ranked = match self.get_top_ranked_peer(&remaining_table) {
                (Some(peer), score) => {
                    if score == u64::MAX {
                        // we expect all remaining peers to score the same u64::MAX score, so we abort.
                        break;
                    } else {
                        peer
                    }
                }
                (None, _) => break, // we have exhausted the peer table, abort.
            };

            selected_table.insert(top_ranked, remaining_table.remove(&top_ranked).unwrap());
            selected_peers.push(top_ranked);

            if selected_peers.len() == self.config.exploitation_target {
                break;
            }

            self.transform_peer_table(&mut selected_table, &mut remaining_table);
        }

        let to_remove_target = if trim_excess_only {
            active_perigee_amount.saturating_sub(self.config.perigee_outbound_target)
        } else {
            usize::max(
                (active_perigee_amount + self.config.exploration_target).saturating_sub(self.config.perigee_outbound_target),
                self.config.exploration_target,
            )
        };

        let mut excused_peers = self.get_excused_peers(&perigee_routers);
        let mut eviction_candidates =
            remaining_table.keys().cloned().filter(|p| !excused_peers.contains(p)).collect::<Vec<_>>();

        if eviction_candidates.len() < to_remove_target {
            while eviction_candidates.len() < to_remove_target {
                if !excused_peers.is_empty() {
                    // We take from excused only if we have to
                    eviction_candidates.push(excused_peers.pop().unwrap());
                } else if !selected_table.is_empty() {
                    // We take from exploited, from lowest to highest impact, as last resort
                    eviction_candidates.push(selected_peers.pop().unwrap());
                }
            }
            (selected_peers, eviction_candidates)
        } else {
            // choose peers to evict from perigee at random from the remaining candidates.
            let eviction_candidates =
                eviction_candidates.choose_multiple(&mut rand::thread_rng(), to_remove_target).cloned().collect();
            (selected_peers, eviction_candidates)
        }
    }

    pub fn start_new_round(&mut self) {
        self.clear();
        self.round_start = Instant::now();
    }

    pub fn should_evaluate(&mut self) -> bool {
        debug!("PerigeeManager: Checking if round should be evaluated: {}", !self.verified_blocks.is_empty());
        !self.verified_blocks.is_empty()
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
            "\n==================== Perigee Statistics ====================\n

         Round Number: {}\n
        
         Number of Perigee Peers:      {}\n
         Total Blocks Seen Count:      {}\n
         Number of Random Graph Peers: {}\n
         Total Blocks Seen Count:      {}\n

         Perigee Config:\n
            Outbound Target:     {}\n
            Exploitation Target: {}\n
            Exploration Target:  {}\n
            Round Duration:  {} secs\n

         Total verified blocks: {}\n
         Total seen blocks: {}\n
         \n
         Block Delivery Race:\n
           Perigee wins:       {} ({:.1}%)\n
           Random Graph wins:  {} ({:.1}%)\n
           Ties:               {} ({:.1}%)\n
         \n
         Perigee Delays (ms):\n
           Count:     {}\n
           Mean:      {:.2}\n
           Median:    {}\n
           Min:       {}\n
           Max:       {}\n
           P90:       {}\n
           P95:       {}\n
           P99:       {}\n
         \n
         Random Graph Delays (ms):\n
           Count:     {}\n
           Mean:      {:.2}\n
           Median:    {}\n
           Min:       {}\n
           Max:       {}\n
           P90:       {}\n
           P95:       {}\n
           P99:       {}\n
         \n
         Comparison:\n
           Mean improvement:   {:.2}ms ({:.1}%)\n
           Median improvement: {}ms ({:.1}%)\n
           P90 improvement:    {}ms ({:.1}%)\n
           \n
         ===========================================================\n",
            self.round_counter,
            number_of_perigee_peers,
            perigee_timestamps.iter().map(|(_, hm)| hm.len()).sum::<usize>(),
            number_of_random_graph_peers,
            random_graph_timestamps.iter().map(|(_, hm)| hm.len()).sum::<usize>(),
            self.config.perigee_outbound_target,
            self.config.exploitation_target,
            self.config.exploration_target,
            self.config.round_frequency * 30,
            self.verified_blocks.len(),
            self.first_seen.len(),
            perigee_wins,
            percentage(perigee_wins, perigee_wins + random_graph_wins + ties),
            random_graph_wins,
            percentage(random_graph_wins, perigee_wins + random_graph_wins + ties),
            ties,
            percentage(ties, perigee_wins + random_graph_wins + ties),
            perigee_stats.count,
            perigee_stats.mean,
            perigee_stats.median,
            perigee_stats.min,
            perigee_stats.max,
            perigee_stats.p90,
            perigee_stats.p95,
            perigee_stats.p99,
            rg_stats.count,
            rg_stats.mean,
            rg_stats.median,
            rg_stats.min,
            rg_stats.max,
            rg_stats.p90,
            rg_stats.p95,
            rg_stats.p99,
            rg_stats.mean as i64 - perigee_stats.mean as i64,
            improvement_percentage(perigee_stats.mean, rg_stats.mean),
            rg_stats.median as i64 - perigee_stats.median as i64,
            improvement_percentage(perigee_stats.median as f64, rg_stats.median as f64),
            rg_stats.p90 as i64 - perigee_stats.p90 as i64,
            improvement_percentage(perigee_stats.p90 as f64, rg_stats.p90 as f64)
        );
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

    fn get_excused_peers(&self, perigee_routers: &[Arc<Router>]) -> Vec<PeerKey> {
        perigee_routers.iter().filter(|r| r.connection_started() > self.round_start).map(|r| r.key()).collect()
    }

    fn rate_peer(&self, values: &[u64]) -> u64 {
        if values.is_empty() {
            // note this is also important so that the return doesn't subtract with overflow.
            return u64::MAX;
        };

        let sorted_values = {
            let mut sv = values.to_owned();
            sv.sort_unstable();
            sv
        };
        sorted_values[
            ((PERCENTILE_RANK * (sorted_values.len() as f64).ceil()) as usize)
            .min(sorted_values.len() - 1) // So we don't out-of-bounds small vecs
            ]
    }

    fn get_top_ranked_peer(&self, peer_table: &HashMap<PeerKey, Vec<u64>>) -> (Option<PeerKey>, u64) {
        let mut best_peer: Option<PeerKey> = None;
        let mut best_score = u64::MAX;
        for (peer, delays) in peer_table.iter() {
            let score = self.rate_peer(delays);
            if score < best_score {
                best_score = score;
                best_peer = Some(*peer);
            }
        }
        debug!(
            "PerigeeManager: Top ranked peer from current peer table is {:?} with score {} - ranked {} values",
            best_peer,
            best_score,
            peer_table.iter().next().unwrap().1.len()
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

        for j in 0..self.verified_blocks.len()  {
            for candidate in candidates.values_mut() {
                // we transform the delay of candidate at pos j to min(candidate_delay_score[j], min(selected_peers_delay_score_at_pos[j])).
                candidate[j] = min(candidate[j], selected_peers.values().map(|vec| vec[j]).min().unwrap());
            }
        }
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
}
