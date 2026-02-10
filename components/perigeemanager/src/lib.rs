use std::{
    cmp::{Ordering, min},
    collections::{HashMap, HashSet, hash_map::Entry},
    fmt::Display,
    net::SocketAddr,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

use itertools::Itertools;
use kaspa_consensus_core::{BlockHashSet, Hash, HashMapCustomHasher};
use kaspa_core::{debug, info, trace};
use kaspa_p2p_lib::{Peer, PeerKey, Router};
use parking_lot::Mutex;
use rand::{Rng, seq::IteratorRandom, thread_rng};

// Tolerance for the number of blocks verified in a round to trigger evaluation.
// For example, at 0.175, if we expect to see 200 blocks verified in a round, but fewer or more than
// 175 or 225 (respectively) are verified, we skip the leverage evaluation for this round.
// The reasoning is that network conditions are not considered stable enough to make a good decision,
// and we would rather skip and wait for the next round.
// Note that exploration can still happen even if this threshold is not met.
// This ensures that we continue to explore in case network conditions are the fault of the connect peers, not network-wide.
const BLOCKS_VERIFIED_FAULT_TOLERANCE: f64 = 0.175;
const IDENT: &str = "PerigeeManager";

/// Holds the score for a peer.
#[derive(Debug)]
pub struct PeerScore {
    p90: u64,
    p95: u64,
    p97_5: u64,
}

impl PeerScore {
    const MAX: PeerScore = PeerScore { p90: u64::MAX, p95: u64::MAX, p97_5: u64::MAX };

    #[inline(always)]
    fn new(p90: u64, p95: u64, p97_5: u64) -> Self {
        PeerScore { p90, p95, p97_5 }
    }
}

impl PartialEq for PeerScore {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        (self.p90, self.p95, self.p97_5) == (other.p90, other.p95, other.p97_5)
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
        (self.p90, self.p95, self.p97_5).cmp(&(other.p90, other.p95, other.p97_5))
    }
}

/// Configuration for the perigee manager.
#[derive(Debug, Clone)]
pub struct PerigeeConfig {
    pub perigee_outbound_target: usize,
    pub leverage_target: usize,
    pub exploration_target: usize,
    pub round_frequency: usize,
    pub round_duration: Duration,
    pub expected_blocks_per_round: u64,
    pub statistics: bool,
    pub persistence: bool,
}

impl PerigeeConfig {
    pub fn new(
        perigee_outbound_target: usize,
        leverage_target: usize,
        exploration_target: usize,
        round_duration: usize,
        connection_manager_tick_duration: Duration,
        statistics: bool,
        persistence: bool,
        bps: u64,
    ) -> Self {
        let expected_blocks_per_round = bps * round_duration as u64;
        let round_duration = Duration::from_secs(round_duration as u64);
        Self {
            perigee_outbound_target,
            leverage_target,
            exploration_target,
            round_frequency: round_duration.as_secs() as usize / connection_manager_tick_duration.as_secs() as usize,
            round_duration,
            expected_blocks_per_round,
            statistics,
            persistence,
        }
    }
}

impl Display for PerigeeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Perigee outbound target: {}, Leverage target: {}, Exploration target: {}, Round duration: {:2} secs, Expected blocks per round: {}, Statistics: {}, Persistence: {}",
            self.perigee_outbound_target,
            self.leverage_target,
            self.exploration_target,
            self.round_duration.as_secs(),
            self.expected_blocks_per_round,
            self.statistics,
            self.persistence
        )
    }
}

#[derive(Debug)]

/// Manages peer selection and scoring.
pub struct PerigeeManager {
    verified_blocks: BlockHashSet, // holds blocks that are consensus verified.
    first_seen: HashMap<Hash, Instant>,
    last_round_leveraged_peers: Vec<PeerKey>,
    round_start: Instant,
    round_counter: u64,
    config: PerigeeConfig,
    is_ibd_running: Arc<AtomicBool>,
}

impl PerigeeManager {
    pub fn new(config: PerigeeConfig, is_ibd_running: Arc<AtomicBool>) -> Mutex<Self> {
        Mutex::new(Self {
            verified_blocks: BlockHashSet::new(),
            first_seen: HashMap::new(),
            last_round_leveraged_peers: Vec::new(),
            round_start: Instant::now(),
            round_counter: 0,
            config,
            is_ibd_running,
        })
    }

    pub fn insert_perigee_timestamp(&mut self, router: &Arc<Router>, hash: Hash, timestamp: Instant, verify: bool) {
        // Inserts and updates the perigee timestamp for the given router
        // and into the local state.
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
        self.last_round_leveraged_peers = peer_keys
    }

    pub fn is_first_round(&self) -> bool {
        self.round_counter == 0
    }

    pub fn trim_peers(&mut self, peers_by_address: Arc<HashMap<SocketAddr, Peer>>) -> Vec<PeerKey> {
        // Contains logic to trim excess perigee peers beyond the configured target
        // without executing a full evaluation.

        debug!("PerigeeManager: Trimming excess peers from perigee");
        let perigee_peers = peers_by_address.values().filter(|p| p.is_perigee()).cloned().collect::<Vec<Peer>>();
        let to_remove_amount = perigee_peers.len().saturating_sub(self.config.perigee_outbound_target);
        let excused_peers = self.get_excused_peers(&perigee_peers);

        perigee_peers
            .iter()
            // Ensure we do not remove leveraged or excused peers
            .filter(|r| !self.last_round_leveraged_peers.contains(&r.key()) && !excused_peers.contains(&r.key()))
            .map(|r| r.key())
            .choose_multiple(&mut thread_rng(), to_remove_amount)
            .iter()
            // In cases where we do not have enough non-excused/non-leveraged peers to remove,
            // we fill the remaining slots with excused peers.
            // Note: We do not expect to ever need to chain with last round's leveraged peers.
            .chain(excused_peers.iter())
            .take(to_remove_amount)
            .cloned()
            .collect()
    }

    pub fn evaluate_round(&mut self, peer_by_address: &HashMap<SocketAddr, Peer>) -> (Vec<PeerKey>, HashSet<PeerKey>, bool) {
        self.round_counter += 1;
        debug!("[{}]: evaluating round: {}", IDENT, self.round_counter);

        let (mut peer_table, perigee_peers) = self.build_table(peer_by_address);

        let is_ibd_running = self.is_ibd_running();

        // First, we excuse all peers with insufficient data this round
        self.excuse(&mut peer_table, &perigee_peers);

        // This excludes peers that have been excused, as well as those that have not provided any data this round.
        let amount_of_contributing_perigee_peers = peer_table.len();
        // In contrast, this is the total number of perigee peers registered in the hub.
        let amount_of_perigee_peers = perigee_peers.len();
        debug!(
            "[{}]: amount_of_perigee_peers: {}, amount_of_contributing_perigee_peers: {}",
            IDENT, amount_of_perigee_peers, amount_of_contributing_perigee_peers
        );
        // For should_leverage, we are conservative and require that we have enough contributing peers for sufficient data.
        let should_leverage = self.should_leverage(is_ibd_running, amount_of_contributing_perigee_peers);
        // For should_explore, we are more aggressive and only require that we have enough total perigee peers.
        // As insufficient data may be malicious behavior by some peers, we prefer to continue churning peers.
        let should_explore = self.should_explore(is_ibd_running, amount_of_perigee_peers);

        let mut has_leveraged_changed = false;

        if !should_leverage && !should_explore {
            // In this case we skip leveraging and exploration this round.
            // We maintain the last round leveraged peers as-is.
            debug!("[{}]: skipping leveraging and exploration this round", IDENT);
            return (self.last_round_leveraged_peers.clone(), HashSet::new(), has_leveraged_changed);
        }

        // i.e. the peers that we mark as "to leverage" this round.
        let selected_peers = if should_leverage {
            let selected_peers = self.leverage(&mut peer_table);
            debug!(
                "[{}]: Selected peers for leveraging this round: {:?}",
                IDENT,
                selected_peers.iter().map(|pk| pk.to_string()).collect_vec()
            );
            // We consider rank changes as well as peer changes here,
            if self.last_round_leveraged_peers != selected_peers {
                // Leveraged peers has changed
                debug!("[{}]: Leveraged peers have changed this round", IDENT);
                has_leveraged_changed = true;
                // Update last round's leveraged peers to the newly selected peers
                self.last_round_leveraged_peers = selected_peers.clone();
            }
            // Return the newly selected peers
            selected_peers
        } else {
            debug!("[{}]: skipping leveraging this round", IDENT);
            // Remove all previously leveraged peers from the peer table to avoid eviction
            for pk in self.last_round_leveraged_peers.iter() {
                peer_table.remove(pk);
            }
            // Return the previous set
            self.last_round_leveraged_peers.clone()
        };

        // i.e. the peers that we mark as "to evict" this round.
        let deselected_peers = if should_explore {
            debug!("[{}]: exploring peers this round", IDENT);
            self.explore(&mut peer_table, amount_of_perigee_peers)
        } else {
            debug!("[{}]: skipping exploration this round", IDENT);
            HashSet::new()
        };

        (selected_peers, deselected_peers, has_leveraged_changed)
    }

    fn leverage(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>) -> Vec<PeerKey> {
        // This is a greedy algorithm, and does not guarantee a globally optimal set of peers.

        // Sanity check
        assert!(peer_table.len() >= self.config.leverage_target, "Potentially entering an endless loop");

        // We use this Vec to maintain track and ordering of selected peers
        let mut selected_peers: Vec<Vec<PeerKey>> = Vec::new();
        let mut num_peers_selected = 0;
        let mut remaining_table;

        // Counts the outer loop only
        let mut i = 0;

        // Outer loop: (re)starts the building of an optimal set of peers from scratch, based on a joint subset scoring mechanism.
        // Note: This potential repetition is not defined in the original Perigee paper, but even with extensive tie-breaking,
        // and with large numbers of perigee peers (i.e., a leverage target > 16), building a single optimal set of peers quickly runs out of peers to select.
        // As such, to ensure we utilize the full leveraging space, we re-run this outer loop
        // to build additional independent complementary sets of peers, thereby reducing reliance on a single such set of peers.
        'outer: while num_peers_selected < self.config.leverage_target {
            debug!(
                "[{}]: Starting new outer loop iteration for leveraging peers, currently selected {} peers",
                IDENT, num_peers_selected
            );

            selected_peers.push(Vec::new());

            // First, we create a new empty selected peer table for this iteration
            let mut selected_table = HashMap::new();

            // We redefine the remaining table for this iteration as a clone of the original peer table
            // Note: If we knew that we would not be re-entering this outer loop, we could avoid this clone.
            remaining_table = peer_table.clone();

            // Start with the last best score as max
            let mut last_score = PeerScore::MAX;

            // Inner loop: This loop selects peers one by one and rates them based on contributions to advancing the current set's joint score,
            // it does this until we reach the leverage target, the available peers are exhausted, or until a local optimum is reached.
            'inner: while num_peers_selected < self.config.leverage_target {
                trace!(
                    "[{}]: New inner loop iteration for leveraging peers, currently selected {} peers",
                    IDENT,
                    selected_peers.get(i).map(|current_set| current_set.len()).unwrap_or(0)
                );

                // Get the top ranked peer from the remaining table
                let (top_ranked, top_ranked_score) = match self.get_top_ranked_peer(&remaining_table) {
                    (Some(peer), score) => (peer, score),
                    _ => {
                        break 'outer; // no more peers to select from
                    }
                };

                if top_ranked_score == last_score {
                    // Break condition: local optimum reached.
                    if top_ranked_score == PeerScore::MAX {
                        // All remaining peers are unrankable; we cannot proceed further.
                        break 'outer;
                    } else {
                        // We have reached a local optimum;
                        if num_peers_selected < self.config.leverage_target {
                            // Build additional sets of leveraged peers
                            break 'inner;
                        } else {
                            break 'outer;
                        }
                    }
                }

                selected_table.insert(top_ranked, remaining_table.remove(&top_ranked).unwrap());
                selected_peers[i].push(top_ranked);
                num_peers_selected += 1;

                if num_peers_selected == self.config.leverage_target {
                    // Reached our target
                    break 'outer;
                } else {
                    // Transform the remaining table accounting also for the newly selected peer
                    self.transform_peer_table(&mut selected_table, &mut remaining_table);
                }
                last_score = top_ranked_score;
            }

            // Remove already selected peers from the global peer table
            for already_selected in selected_peers[i].iter() {
                peer_table.remove(already_selected);
            }

            i += 1;
        }

        for already_selected in selected_peers[i].iter() {
            peer_table.remove(already_selected);
        }

        if num_peers_selected < self.config.leverage_target {
            // choose randomly from remaining peers to fill the gap
            let to_choose = self.config.leverage_target - num_peers_selected;
            debug!("[{}]: Leveraging did not reach intended target, randomly selecting {} remaining peers", IDENT, to_choose);
            let random_keys: Vec<PeerKey> =
                peer_table.keys().choose_multiple(&mut thread_rng(), to_choose).into_iter().copied().collect();

            for pk in random_keys {
                selected_peers[i].push(pk);
                peer_table.remove(&pk);
            }
        }

        selected_peers.into_iter().flatten().collect()
    }

    fn excuse(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>, perigee_peers: &[Peer]) {
        // Removes excused peers from the peer table so they are not considered for eviction.
        for k in self.get_excused_peers(perigee_peers) {
            peer_table.remove(&k);
        }
    }

    fn explore(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>, amount_of_active_perigee: usize) -> HashSet<PeerKey> {
        // This is conceptually simple: we randomly choose peers to evict from the passed peer table.
        // It is expected that other logic, such as leveraging and excusing peers, has already been applied to the peer table.
        let to_remove_target = std::cmp::min(
            self.config.exploration_target,
            amount_of_active_perigee.saturating_sub(self.config.perigee_outbound_target - self.config.exploration_target),
        );

        peer_table.keys().choose_multiple(&mut thread_rng(), to_remove_target).into_iter().cloned().collect()
    }

    pub fn start_new_round(&mut self) {
        // Clears state and starts a new round timer
        self.clear();
        self.round_start = Instant::now();
    }

    pub fn config(&self) -> PerigeeConfig {
        self.config.clone()
    }

    fn maybe_insert_first_seen(&mut self, hash: Hash, timestamp: Instant) {
        // Inserts the first-seen timestamp for a block if it is earlier than the existing one
        // or if it does not exist yet.
        match self.first_seen.entry(hash) {
            Entry::Occupied(mut o) => {
                let current = o.get_mut();
                if timestamp.lt(current) {
                    *current = timestamp;
                }
            }
            Entry::Vacant(v) => {
                v.insert(timestamp);
            }
        }
    }

    fn verify_block(&mut self, hash: Hash) {
        // Marks a block as verified for this round.
        // I.e., this block will be considered in the current round's evaluation.
        self.verified_blocks.insert(hash);
    }

    fn clear(&mut self) {
        // Resets state for a new round
        debug!("[{}]: Clearing state for new round", IDENT);
        self.verified_blocks.clear();
        self.first_seen.clear();
    }

    fn get_excused_peers(&self, perigee_peers: &[Peer]) -> Vec<PeerKey> {
        // Define excused peers as those that joined perigee after the round started.
        // They should not be penalized for not having enough data in this round.
        // We also sort them by connection time to give more trimming security to the longest connected peers first,
        // This allows them more time to complete a full round.
        perigee_peers
            .iter()
            .sorted_by_key(|p| p.connection_started())
            .filter(|p| p.connection_started() > self.round_start)
            .map(|p| p.key())
            .collect()
    }

    fn rate_peer(&self, values: &[u64]) -> PeerScore {
        // Rates a peer based on its transformed delay values

        if values.is_empty() {
            return PeerScore::MAX;
        }

        // Sort values for percentile calculations
        let sorted_values = {
            let mut sv = values.to_owned();
            sv.sort_unstable();
            sv
        };

        let len = sorted_values.len();

        // This is defined as the scoring mechanism in the corresponding original perigee paper.
        // It favors good connectivity to the bulk of the network while still considering tail-end delays.
        let p90 = sorted_values[((0.90 * len as f64) as usize).min(len - 1)];

        // This is a deviation from the paper;
        // We rate beyond the p90 to tie-break
        // Testing has shown that full coverage of the p90 range often only requires ~4-6 perigee peers.
        // This leaves remaining perigee peers without contribution to latency reduction.
        // As such, we rate these even deeper into the tail-end delays to try to increase coverage of outlier blocks.
        let p95 = sorted_values[((0.95 * len as f64) as usize).min(len - 1)];
        let p97_5 = sorted_values[((0.975 * len as f64) as usize).min(len - 1)];
        // Beyond p97_5 might be too sensitive to noise.

        PeerScore::new(p90, p95, p97_5)
    }

    fn get_top_ranked_peer(&self, peer_table: &HashMap<PeerKey, Vec<u64>>) -> (Option<PeerKey>, PeerScore) {
        // Finds the peer with the best score in the given peer table
        let mut best_peer: Option<PeerKey> = None;
        let mut best_score = PeerScore::MAX;
        let mut tied_count = 0;

        for (peer, delays) in peer_table.iter() {
            let score = self.rate_peer(delays);
            if score < best_score {
                best_score = score;
                best_peer = Some(*peer);
            } else if score == best_score {
                tied_count += 1;
                // Randomly replace with probability 1/tied_count
                // This ensures we don't choose peers based on iteration / HashMap order
                if thread_rng().gen_ratio(1, tied_count) {
                    best_peer = Some(*peer);
                }
            }
        }

        debug!(
            "[{}]: Top ranked peer from current peer table is {:?} with score p90: {}, p95: {}, p97.5: {}",
            IDENT, best_peer, best_score.p90, best_score.p95, best_score.p97_5,
        );
        (best_peer, best_score)
    }

    fn transform_peer_table(&self, selected_peers: &mut HashMap<PeerKey, Vec<u64>>, candidates: &mut HashMap<PeerKey, Vec<u64>>) {
        // Transforms the candidate peer table to min(selected peers' delay scores, candidate delay scores)
        // for each delay score. This is one of the key components of the Perigee algorithm for joint subset selection.

        debug!("[{}]: Transforming peer table", IDENT);

        for j in 0..self.verified_blocks.len() {
            let selected_min_j = selected_peers.values().map(|vec| vec[j]).min().unwrap();
            for candidate in candidates.values_mut() {
                // We transform the delay of candidate at position j to min(candidate_delay_score[j], min(selected_peers_delay_score_at_pos[j])).
                candidate[j] = min(candidate[j], selected_min_j);
            }
        }
    }

    fn should_leverage(&self, is_ibd_running: bool, amount_of_contributing_perigee_peers: usize) -> bool {
        // Conditions that need to be met to trigger leveraging:

        // 1. IBD is not running
        !is_ibd_running &&
        // 2. Sufficient blocks have been verified this round
        self.block_threshold_reached() &&
        // 3. We have enough contributing perigee peers to choose from
        amount_of_contributing_perigee_peers >= self.config.leverage_target
    }

    fn should_explore(&self, is_ibd_running: bool, amount_of_perigee_peers: usize) -> bool {
        // Conditions that should trigger exploration:

        // 1. IBD is not running
        !is_ibd_running &&
        // 2. We are within bounds to evict at least one peer - else we prefer to wait on more peers joining perigee first.
        amount_of_perigee_peers > (self.config.perigee_outbound_target - self.config.exploration_target)
    }

    fn block_threshold_reached(&self) -> bool {
        // Checks whether the amount of verified blocks this round is within the expected bounds to consider leveraging.
        // If this is not the case, the node is likely experiencing network issues, and we rather skip leveraging this round.
        let verified_count = self.verified_blocks.len();
        let expected_count = self.config.expected_blocks_per_round;
        let lower_bound = (expected_count as f64 * (1.0 - BLOCKS_VERIFIED_FAULT_TOLERANCE)) as usize;
        let upper_bound = (expected_count as f64 * (1.0 + BLOCKS_VERIFIED_FAULT_TOLERANCE)) as usize;
        debug!(
            "[{}]: block_threshold_reached: verified_count={}, expected_count={}, lower_bound={}, upper_bound={}",
            IDENT, verified_count, expected_count, lower_bound, upper_bound
        );
        verified_count >= lower_bound && verified_count <= upper_bound
    }

    fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn iterate_verified_first_seen(&self) -> impl Iterator<Item = (&Hash, &Instant)> {
        // Iterates over first_seen entries that correspond to verified blocks only.
        self.first_seen.iter().filter(move |(hash, _)| self.verified_blocks.contains(hash))
    }

    fn build_table(&self, peer_by_address: &HashMap<SocketAddr, Peer>) -> (HashMap<PeerKey, Vec<u64>>, Vec<Peer>) {
        // Builds the peer delay table for all perigee peers.
        debug!("[{}]: Building peer table", IDENT);
        let mut peer_table: HashMap<PeerKey, Vec<u64>> = HashMap::new();

        // Pre-fetch perigee timestamps for all perigee peers.
        // Calling the .perigee_timestamps() method in the loop would become expensive.
        let mut perigee_timestamps = HashMap::new();
        let mut perigee_peers = Vec::new();
        for p in peer_by_address.values() {
            if p.is_perigee() {
                perigee_timestamps.insert(p.key(), p.perigee_timestamps());
                perigee_peers.push(p.clone());
            }
        }

        for (hash, first_ts) in self.iterate_verified_first_seen() {
            for (peer_key, peer_timestamps) in perigee_timestamps.iter_mut() {
                let mut timestamps = peer_timestamps.as_ref().clone();
                match timestamps.entry(*hash) {
                    Entry::Occupied(o) => {
                        let delay = o.get().duration_since(*first_ts).as_millis() as u64;
                        peer_table.entry(*peer_key).or_default().push(delay);
                    }
                    Entry::Vacant(_) => {
                        // Peer did not report this block this round; assign max delay
                        peer_table.entry(*peer_key).or_default().push(u64::MAX);
                    }
                }
            }
        }
        (peer_table, perigee_peers)
    }

    pub fn log_statistics(&self, peer_by_address: &HashMap<SocketAddr, Peer>) {
        // Note: this function has been artificially compressed for code-sparsity, as it is not mission critical, but is rather verbose.
        let (perigee_ts, rg_ts): (Vec<_>, Vec<_>) =
            peer_by_address.values().filter(|p| p.is_perigee() || p.is_random_graph()).partition_map(|p| {
                if p.is_perigee() {
                    itertools::Either::Left((p.key(), p.perigee_timestamps()))
                } else {
                    itertools::Either::Right((p.key(), p.perigee_timestamps()))
                }
            });

        let (mut p_delays, mut rg_delays, mut p_wins, mut rg_wins, mut ties) = (vec![], vec![], 0usize, 0usize, 0usize);

        for (hash, ts) in self.iterate_verified_first_seen() {
            let p_d = perigee_ts.iter().filter_map(|(_, hm)| hm.get(hash).map(|t| t.duration_since(*ts).as_millis() as u64)).min();
            let rg_d = rg_ts.iter().filter_map(|(_, hm)| hm.get(hash).map(|t| t.duration_since(*ts).as_millis() as u64)).min();
            match (p_d, rg_d) {
                (Some(p), Some(rg)) => {
                    p_delays.push(p);
                    rg_delays.push(rg);
                    match p.cmp(&rg) {
                        Ordering::Less => p_wins += 1,
                        Ordering::Greater => rg_wins += 1,
                        Ordering::Equal => ties += 1,
                    }
                }
                (Some(p), None) => {
                    p_delays.push(p);
                    p_wins += 1;
                }
                (None, Some(rg)) => {
                    rg_delays.push(rg);
                    rg_wins += 1;
                }
                _ => {}
            }
        }

        if p_delays.is_empty() && rg_delays.is_empty() {
            debug!("PerigeeManager Statistics: No data available for this round");
            return;
        }

        let stats = |d: &mut [u64]| -> (usize, f64, u64, u64, u64, u64, u64, u64) {
            if d.is_empty() {
                return (0, 0.0, 0, 0, 0, 0, 0, 0);
            }
            d.sort_unstable();
            let n = d.len();
            let pct = |p: f64| d[((n as f64 * p) as usize).min(n - 1)];
            (n, d.iter().sum::<u64>() as f64 / n as f64, d[n / 2], d[0], d[n - 1], pct(0.90), pct(0.95), pct(0.99))
        };

        let (pc, pm, pmed, pmin, pmax, p90, p95, p99) = stats(&mut p_delays);
        let (rc, rm, rmed, rmin, rmax, r90, r95, r99) = stats(&mut rg_delays);
        let total = p_wins + rg_wins + ties;
        let pct = |p, t| if t == 0 { 0.0 } else { p as f64 / t as f64 * 100.0 };
        let imp = |p: f64, r: f64| if r == 0.0 { 0.0 } else { (r - p) / r * 100.0 };

        info!(
            "[{}]\n\
     ════════════════════════════════════════════════════════════════════════════ \n\
                           PERIGEE STATISTICS - Round {:4}                     \n\
     ════════════════════════════════════════════════════════════════════════════ \n\
      Config: Out={:<2} Leverage={:<2} Explore={:<2} Duration={:<5}s                   \n\
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
            IDENT,
            self.round_counter,
            self.config.perigee_outbound_target,
            self.config.leverage_target,
            self.config.exploration_target,
            self.config.round_duration.as_secs(),
            perigee_ts.len(),
            perigee_ts.iter().map(|(_, hm)| hm.len()).sum::<usize>(),
            rg_ts.len(),
            rg_ts.iter().map(|(_, hm)| hm.len()).sum::<usize>(),
            self.verified_blocks.len(),
            self.first_seen.len(),
            p_wins,
            pct(p_wins, total),
            rg_wins,
            pct(rg_wins, total),
            ties,
            pct(ties, total),
            pc,
            rc,
            pm,
            rm,
            rm - pm,
            imp(pm, rm),
            pmed,
            rmed,
            rmed as i64 - pmed as i64,
            imp(pmed as f64, rmed as f64),
            pmin,
            rmin,
            pmax,
            rmax,
            p90,
            r90,
            r90 as i64 - p90 as i64,
            imp(p90 as f64, r90 as f64),
            p95,
            r95,
            p99,
            r99
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::params::TESTNET_PARAMS;
    use kaspa_hashes::Hash;
    use kaspa_p2p_lib::PeerOutboundType;
    use kaspa_p2p_lib::test_utils::RouterTestExt;
    use kaspa_utils::networking::PeerId;

    use std::collections::HashMap;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;
    use uuid::Uuid;

    /// Generates a unique Router wit incremental IPv4 SocketAddr and PeerId for testing purposes.
    fn generate_unique_router(time_connected: Instant) -> std::sync::Arc<kaspa_p2p_lib::Router> {
        static ROUTER_COUNTER: AtomicU64 = AtomicU64::new(1);

        let id = ROUTER_COUNTER.fetch_add(1, Ordering::Relaxed);
        let ip_seed = id;
        let octet1 = ((ip_seed >> 24) & 0xFF) as u8;
        let octet2 = ((ip_seed >> 16) & 0xFF) as u8;
        let octet3 = ((ip_seed >> 8) & 0xFF) as u8;
        let octet4 = (ip_seed & 0xFF) as u8;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(octet1, octet2, octet3, octet4)), TESTNET_PARAMS.default_p2p_port());
        let peer_id = PeerId::new(Uuid::from_u128(id as u128));
        RouterTestExt::test_new(peer_id, addr, Some(PeerOutboundType::Perigee), time_connected)
    }

    // Helper to generate a default PerigeeConfig for testing purposes
    fn generate_config() -> PerigeeConfig {
        PerigeeConfig::new(8, 4, 2, 30, std::time::Duration::from_secs(30), true, true, TESTNET_PARAMS.bps())
    }

    // Helper to generate a globally unique block hash
    fn generate_unique_block_hash() -> Hash {
        static HASH_COUNTER: AtomicU64 = AtomicU64::new(1);

        Hash::from_u64_word(HASH_COUNTER.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn test_insertions() {
        let routers = (0..2).map(|_| generate_unique_router(Instant::now())).collect::<Vec<_>>();
        let manager = PerigeeManager::new(generate_config(), Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let now: Vec<_> = (0..4).map(|_| Instant::now()).collect();
        let block_hashes: Vec<_> = (0..4).map(|_| generate_unique_block_hash()).collect();
        for (i, (now, block_hash)) in now.iter().zip(block_hashes.iter()).enumerate() {
            manager.lock().insert_perigee_timestamp(&routers[(i + 1) % 2].clone(), *block_hash, *now, (i + 1) % 2 == 0);
        }

        let manager = manager.lock();
        // Check first_seen and router timestamps
        for (i, (now, block_hash)) in now.iter().zip(block_hashes.iter()).enumerate() {
            let ts = manager.first_seen.get(block_hash).unwrap();
            assert_eq!(ts, now);
            // Only the router that received the block should have it, and timestamp should match
            let idx = (i + 1) % 2;
            if idx == 0 {
                assert!(manager.verified_blocks.contains(block_hash), "Block should be verified for even indices");
            } else {
                assert!(!manager.verified_blocks.contains(block_hash), "Block should not be verified for odd indices");
            }
            let perigee_timestamps = &routers[idx].perigee_timestamps();
            let router_ts = perigee_timestamps.get(block_hash).unwrap();
            assert_eq!(router_ts, now, "Router's perigee_timestamps should match inserted timestamp");
            assert_eq!(router_ts, ts, "Router's perigee_timestamps should match manager's first_seen");
            // The other router should NOT have this block_hash
            let other_perigee_timestamps = &routers[1 - idx].perigee_timestamps();
            assert!(!other_perigee_timestamps.contains_key(block_hash), "Other router should not have this block hash");
        }

        // Check lengths
        assert_eq!(manager.first_seen.len(), block_hashes.len(), "first_seen should have all inserted blocks");
        assert_eq!(manager.verified_blocks.len(), block_hashes.len().div_ceil(2), "verified_blocks should have half the blocks");
        for router in &routers {
            let perigee_timestamps = router.perigee_timestamps();
            assert_eq!(perigee_timestamps.len(), block_hashes.len() / 2, "Each router should have half the block hashes");
        }
    }

    #[test]
    fn test_trim_peers() {
        // Set-up environment
        let config = generate_config();
        let manager = PerigeeManager::new(config.clone(), Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let leverage_target = config.leverage_target;
        let perigee_outbound_target = config.perigee_outbound_target;
        let excused_count = perigee_outbound_target + 1 - leverage_target;
        // Set up so that all non-leveraged, non-excused peers are needed to fill the outbound target, so only one excused peer can be trimmed
        let total_peers = leverage_target + excused_count;

        // Create leveraged peers (should not be trimmed)
        let now = Instant::now() - std::time::Duration::from_secs(3600);
        let mut routers = Vec::new();
        for _ in 0..leverage_target {
            routers.push(generate_unique_router(now));
        }

        // Create excused peers, joined after round start, should be excused and and only trimmed as a last resort (ordered by connection time)
        let mut excused_routers = Vec::new();
        for i in 0..excused_count {
            let t = now + std::time::Duration::from_secs(10 + i as u64);
            excused_routers.push(generate_unique_router(t));
        }

        // Build all peers
        let mut peers = HashMap::new();
        for router in routers.iter().chain(excused_routers.iter()) {
            let peer = Peer::from((&**router, true));
            peers.insert(router.key(), peer.clone());
        }

        // Build peer_by_addr
        let mut peer_by_addr = HashMap::new();
        for peer in peers.values() {
            peer_by_addr.insert(peer.net_address(), peer.clone());
        }

        // Set leveraged and excused peers in manager
        manager.lock().set_initial_persistent_peers(routers.iter().map(|r| r.key()).collect());
        manager.lock().round_start = now; // Set round start to 'now' for excused logic

        // Call trim_peers
        let to_remove = manager.lock().trim_peers(Arc::new(peer_by_addr));

        // Assert correct number trimmed
        let expected_trim = total_peers - perigee_outbound_target;
        assert_eq!(to_remove.len(), expected_trim, "Should trim down to perigee_outbound_target");

        // Assert no leveraged peer is trimmed
        let leveraged_keys: Vec<_> = routers.iter().map(|r| r.key()).collect();
        for k in &to_remove {
            assert!(!leveraged_keys.contains(k), "Leveraged peer should not be trimmed");
        }

        // Assert that exactly one excused peer is trimmed (evicted), and the rest are not
        let excused_keys: Vec<_> = excused_routers.iter().map(|r| r.key()).collect();
        let excused_trimmed: Vec<_> = excused_keys.iter().filter(|k| to_remove.contains(k)).collect();
        assert_eq!(excused_trimmed.len(), 1, "Exactly one excused peer should be trimmed as a last resort");
        // The rest of the excused peers should not be trimmed
        let excused_not_trimmed: Vec<_> = excused_keys.iter().filter(|k| !to_remove.contains(k)).collect();
        assert_eq!(excused_not_trimmed.len(), excused_count - 1, "All but one excused peer should remain");
        // Check excused ordering by connection time (still valid for remaining excused)
        let mut excused_peers: Vec<_> = excused_routers.iter().map(|r| peers.get(&r.key()).unwrap()).collect();
        excused_peers.sort_by_key(|p| p.connection_started());
        for w in excused_peers.windows(2) {
            assert!(w[0].connection_started() <= w[1].connection_started(), "Excused peers should be ordered by connection time");
        }
    }

    #[test]
    fn test_peer_rating() {
        let score = (0..1000).collect::<Vec<u64>>();
        let manager = PerigeeManager::new(generate_config(), Arc::new(std::sync::atomic::AtomicBool::new(false)));
        let peer_score = manager.lock().rate_peer(&score);
        let expected_peer_score = PeerScore::new(900, 950, 975);
        assert_eq!(peer_score, expected_peer_score);
    }

    #[test]
    fn test_perigee_round_leverage_and_eviction() {
        run_round(false);
    }

    #[test]
    fn test_perigee_round_skips_while_ibd_running() {
        run_round(true);
    }

    fn run_round(ibd_running: bool) {
        kaspa_core::log::try_init_logger("debug");

        // Set up environment
        let is_ibd_running = Arc::new(std::sync::atomic::AtomicBool::new(ibd_running));
        let mut config = generate_config();
        let now = Instant::now() - std::time::Duration::from_secs(3600);
        let peer_count = config.perigee_outbound_target;
        let blocks_per_router = 300;
        config.expected_blocks_per_round = blocks_per_router as u64;
        let manager = PerigeeManager::new(config, is_ibd_running);
        let mut routers = Vec::new();
        for _ in 0..peer_count {
            let router = generate_unique_router(now);
            routers.push(router);
        }

        // Insert blocks using a deterministic delay pattern via bucketing ts
        let leverage_target = manager.lock().config.leverage_target;
        for block_idx in 0..blocks_per_router {
            let block_hash = generate_unique_block_hash();
            let base_ts = now + std::time::Duration::from_millis((block_idx as u64) * 10_000);
            for (i, router) in routers.iter().enumerate() {
                let ts = if i < leverage_target {
                    let bucket_start = (i as u64) * 10;
                    let delay = bucket_start + (block_idx as u64 % 10);
                    base_ts + std::time::Duration::from_millis(delay)
                } else {
                    base_ts + std::time::Duration::from_millis(100_000 + (i as u64) * 10)
                };
                manager.lock().insert_perigee_timestamp(router, block_hash, ts, true);
            }
        }

        assert!(manager.lock().verified_blocks.len() == blocks_per_router);

        // Build peers and peer_by_addr after all timestamps are inserted
        let mut peers = HashMap::new();
        for router in &routers {
            let peer = Peer::from((&**router, true));
            peers.insert(router.key(), peer.clone());
        }
        let mut peer_by_addr = HashMap::new();
        for peer in peers.values() {
            peer_by_addr.insert(peer.net_address(), peer.clone());
        }

        // Execute a perigee round
        let (leveraged, evicted, has_leveraged_changed) = manager.lock().evaluate_round(&peer_by_addr);
        debug!("Leveraged peers: {:?}", leveraged);
        debug!("Evicted peers: {:?}", evicted);

        // Perform assertions:
        if ibd_running {
            // While IBD is running, no leveraging or eviction should occur
            assert!(!has_leveraged_changed, "Leveraging should be skipped while IBD is running");
            assert!(leveraged.is_empty(), "No peers should be leveraged while IBD is running");
            assert!(evicted.is_empty(), "No peers should be evicted while IBD is running");
            return;
        };
        assert!(has_leveraged_changed, "Leveraging should not be skipped in this test");
        assert_eq!(
            leveraged,
            routers.iter().take(leverage_target).map(|r| r.key()).collect::<Vec<PeerKey>>(),
            "Leverage set should match actual deterministic selection (order and membership)"
        );
        // No leveraged peer should be evicted
        assert!(leveraged.iter().all(|p| !evicted.contains(p)), "No leveraged peer should be evicted");
        assert_eq!(evicted.len(), manager.lock().config.exploration_target);

        // Reset round.
        manager.lock().start_new_round();
        // Ensure state is cleared.
        assert!(manager.lock().verified_blocks.is_empty(), "Verified blocks should be cleared after starting new round");
        assert!(manager.lock().first_seen.is_empty(), "First seen timestamps should be cleared after starting new round");
    }
}
