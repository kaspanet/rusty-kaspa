use std::{
    cmp::{min, Ordering},
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt::Display,
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use itertools::Itertools;
use kaspa_consensus_core::{BlockHashSet, Hash, HashMapCustomHasher};
use kaspa_core::{info, trace};
use kaspa_p2p_lib::{Hub, Peer, PeerKey, Router};
use log::debug;
use parking_lot::Mutex;
use rand::{seq::IteratorRandom, thread_rng, Rng};

// Tolerance for the number of blocks verified in a round to trigger evaluation.
// For example, at 0.175, if we expect to see 200 blocks verified in a round, but fewer or more than
// 175 or 225 (respectively) are verified, we skip the leverage evaluation for this round.
// The reasoning is that network conditions are not considered stable enough to make a good decision,
// and we would rather skip and wait for the next round.
// Note that exploration can still happen even if this threshold is not met.
// This ensures that we continue to explore in case network conditions are the fault of the peers, not oneself.
const BLOCKS_VERIFIED_FAULT_TOLERANCE: f64 = 0.175;
const IDENT: &str = "PerigeeManager";

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
    const MAX: PeerScore = PeerScore {
        p90: u64::MAX,
        p95: u64::MAX,
        p97_5: u64::MAX,
        p98_25: u64::MAX,
        p99_125: u64::MAX,
        p99_6875: u64::MAX,
        p100: u64::MAX,
    };

    #[inline(always)]
    fn new(p90: u64, p95: u64, p97_5: u64, p98_25: u64, p99_125: u64, p99_6875: u64, p100: u64) -> Self {
        PeerScore { p90, p95, p97_5, p98_25, p99_125, p99_6875, p100 }
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
pub struct PerigeeManager {
    verified_blocks: BlockHashSet, // holds blocks that are consensus verified.
    first_seen: HashMap<Hash, Instant>,
    last_round_leveraged_peers: Vec<PeerKey>,
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
            last_round_leveraged_peers: Vec::new(),
            round_start: Instant::now(),
            round_counter: 0,
            config,
            hub,
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
        let perigee_routers = self
            .hub
            .perigee_routers()
            .into_iter()
            // This filtering is important to ensure we trim based on the passed peers_by_address snapshot,
            // not the current state of the hub, which may have changed since the snapshot was taken.
            .filter(|r| peers_by_address.contains_key(&SocketAddr::new(r.net_address().ip(), r.net_address().port())))
            .collect::<Vec<_>>();
        let to_remove_amount = perigee_routers.len().saturating_sub(self.config.perigee_outbound_target);
        let excused_peers = self.get_excused_peers(&perigee_routers);

        perigee_routers
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

    pub fn evaluate_round(&mut self) -> (Vec<PeerKey>, HashSet<PeerKey>, bool) {
        self.round_counter += 1;
        debug!("[{}]: evaluating round: {}", IDENT, self.round_counter);

        let (mut peer_table, perigee_routers) = self.build_table();

        let is_ibd_running = self.is_ibd_running();

        // First, we excuse all peers with insufficient data this round
        self.excuse(&mut peer_table, &perigee_routers);

        // This excludes peers that have been excused, as well as those that have not provided any data this round.
        let amount_of_contributing_perigee_peers = peer_table.len();
        // In contrast, this is the total number of perigee peers registered in the hub.
        let amount_of_perigee_peers = perigee_routers.len();
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

    fn excuse(&self, peer_table: &mut HashMap<PeerKey, Vec<u64>>, perigee_routers: &[Arc<Router>]) {
        // Removes excused peers from the peer table so they are not considered for eviction.
        for k in self.get_excused_peers(perigee_routers) {
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
        // Define excused peers as those that joined perigee after the round started.
        // They should not be penalized for not having enough data in this round.
        // We also sort them by connection time to give more trimming security to the longest connected peers first,
        // This allows them more time to complete a full round.
        perigee_routers
            .iter()
            .sorted_by_key(|r| r.connection_started())
            .filter(|r| r.connection_started() > self.round_start)
            .map(|r| r.key())
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
        let p98_25 = sorted_values[((0.9825 * len as f64) as usize).min(len - 1)];
        let p99_125 = sorted_values[((0.99125 * len as f64) as usize).min(len - 1)];
        let p99_6875 = sorted_values[((0.996875 * len as f64) as usize).min(len - 1)];
        let p100 = sorted_values[len - 1];

        PeerScore::new(p90, p95, p97_5, p98_25, p99_125, p99_6875, p100)
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
            "[{}]: Top ranked peer from current peer table is {:?} with score p90: {}, p95: {}, p97.5: {}, p98.25: {}, p99.125: {}, p99.6875: {}, p.100: {}",
            IDENT,
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
        verified_count >= lower_bound && verified_count <= upper_bound
    }

    fn is_ibd_running(&self) -> bool {
        self.is_ibd_running.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn iterate_verified_first_seen(&self) -> impl Iterator<Item = (&Hash, &Instant)> {
        // Iterates over first_seen entries that correspond to verified blocks only.
        self.first_seen.iter().filter(move |(hash, _)| self.verified_blocks.contains(hash))
    }

    fn build_table(&self) -> (HashMap<PeerKey, Vec<u64>>, Vec<Arc<Router>>) {
        // Builds the peer delay table for all perigee routers.
        debug!("[{}]: Building peer table", IDENT);

        let mut peer_table: HashMap<PeerKey, Vec<u64>> = HashMap::new();
        let perigee_routers = self.hub.perigee_routers();

        // The below is important as we clone out the HashMap; this should only be done once per round.
        // Calling the .perigee_timestamps() method in the loop would become expensive.
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
        // Note: this function has been artificially compressed for code-sparsity, as it is not mission critical, but is rather verbose.
        let perigee_ts: Vec<_> = self.hub.perigee_routers().iter().map(|r| (r.key(), r.perigee_timestamps())).collect();
        let rg_ts: Vec<_> = self.hub.random_graph_routers().iter().map(|r| (r.key(), r.perigee_timestamps())).collect();

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
    use kaspa_consensus_core::Hash;
    use kaspa_p2p_lib::test_utils::{HubTestExt, RouterTestExt};
    use kaspa_p2p_lib::{Hub, PeerOutboundType, Router};
    use kaspa_utils::networking::PeerId;

    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Instant;
    use uuid::Uuid;

    /// Generates a unique PeerKey and incremental IPv4 SocketAddr for testing purposes.
    fn generate_unique_router(time_connected: Instant) -> Arc<Router> {
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

    fn generate_config() -> PerigeeConfig {
        PerigeeConfig::new(8, 4, 2, 30, std::time::Duration::from_secs(30), true, true, TESTNET_PARAMS.bps())
    }

    #[test]
    fn test_insert_timestamp() {
        // Create a test router with a unique key and outbound type
        let router = generate_unique_router(Instant::now());

        // Create a mock hub
        let hub = Hub::default();

        // Create PerigeeManager
        let config = generate_config();
        let is_ibd_running = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut manager = PerigeeManager::new(hub, config, is_ibd_running).into_inner();

        // Timestamps
        let ts_1 = Instant::now();
        let ts_2 = Instant::now(); // this one is later.

        // Assert the timestamp was inserted, and aligns, correctly.
        let hash = Hash::from_u64_word(1);
        manager.insert_perigee_timestamp(&router, hash, ts_2, false);
        assert_eq!(router.get_perigee_timestamps().len(), 1);
        assert_eq!(manager.first_seen.len(), 1);
        assert!(manager.verified_blocks.is_empty()); // verify was false
        assert_eq!(router.get_perigee_timestamps().get(&hash), Some(&ts_2));
        assert_eq!(manager.first_seen.get(&hash), Some(&ts_2));

        // Assert new timestamp overrides the existing one with an earlier timestamp
        let hash = Hash::from_u64_word(1);
        manager.insert_perigee_timestamp(&router, hash, ts_1, true);
        assert_eq!(router.get_perigee_timestamps().len(), 1);
        assert_eq!(manager.first_seen.len(), 1);
        assert_eq!(manager.verified_blocks.len(), 1); // verify was true
        assert_eq!(manager.verified_blocks.get(&hash), Some(&hash));
        assert_eq!(router.get_perigee_timestamps().get(&hash), Some(&ts_1));
        assert_eq!(manager.first_seen.get(&hash), Some(&ts_1));

        // Assert that a new hash ts is added correctly
        let hash2 = Hash::from_u64_word(2);
        manager.insert_perigee_timestamp(&router, hash2, ts_2, true);
        assert_eq!(router.get_perigee_timestamps().len(), 2);
        assert_eq!(manager.first_seen.len(), 2);
        assert_eq!(manager.verified_blocks.len(), 2); // verify was true
        assert_eq!(manager.verified_blocks.get(&hash2), Some(&hash2));
        assert_eq!(router.get_perigee_timestamps().get(&hash2), Some(&ts_2));
        assert_eq!(manager.first_seen.get(&hash2), Some(&ts_2));
    }

    #[test]
    fn test_is_first_round() {
        let hub = Hub::default();
        let config = generate_config();
        let is_ibd_running = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut manager = PerigeeManager::new(hub, config, is_ibd_running).into_inner();

        assert!(manager.is_first_round());

        manager.round_counter = 1;
        assert!(!manager.is_first_round());
    }

    #[test]
    fn test_rate_peer() {
        let hub = Hub::default();
        let config = generate_config();
        let is_ibd_running = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let manager = PerigeeManager::new(hub, config, is_ibd_running).into_inner();

        let delays = (0..1_000_001).collect::<Vec<_>>();
        let score = manager.rate_peer(&delays);
        assert_eq!(score.p90, 900_000);
        assert_eq!(score.p95, 950_000);
        assert_eq!(score.p97_5, 975_000);
        assert_eq!(score.p98_25, 982_500);
        assert_eq!(score.p99_125, 991_250);
        assert_eq!(score.p99_6875, 996_875);
        assert_eq!(score.p100, 1_000_000);
    }

    #[test]
    fn test_start_new_round_resets_state() {
        let hub = Hub::default();
        let config = generate_config();
        let is_ibd_running = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut manager = PerigeeManager::new(hub, config, is_ibd_running).into_inner();

        manager.verified_blocks.insert(Hash::from_u64_word(1));
        manager.first_seen.insert(Hash::from_u64_word(1), Instant::now());
        manager.start_new_round();
        assert!(manager.verified_blocks.is_empty());
    }
}
