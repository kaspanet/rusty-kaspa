use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::Instant,
    u64,
};

use kaspa_consensus_core::{BlockHashMap, BlockHashSet, Hash, HashMapCustomHasher};
use kaspa_p2p_lib::{Hub, Peer, PeerKey, Router};
use log::debug;
use parking_lot::Mutex;
use rand::{rngs::ThreadRng, seq::SliceRandom};

const PERCENTILE_RANK: f64 = 0.9;

#[derive(Debug, Clone)]
pub struct PerigeeConfig {
    pub perigee_outbound_target: usize,
    pub exploitation_target: usize,
    pub exploration_target: usize,
    pub min_round_duration_in_secs: u64,
}

impl PerigeeConfig {
    pub fn new(
        perigee_outbound_target: usize,
        exploitation_target: usize,
        exploration_target: usize,
        min_round_duration_in_secs: u64,
    ) -> Self {
        Self { perigee_outbound_target, exploitation_target, exploration_target, min_round_duration_in_secs }
    }

    pub fn should_initiate_perigee(&self) -> bool {
        self.perigee_outbound_target > 0 && self.exploration_target > 0 && self.exploitation_target < self.perigee_outbound_target
    }
}

#[derive(Debug)]
pub struct PerigeeManager {
    verified_blocks: BlockHashSet, // holds blocks that are consensus verified.
    first_seen: HashMap<Hash, Instant>,
    round_start: Instant,
    config: PerigeeConfig,
    hub: Hub,
}

impl PerigeeManager {
    pub fn new(hub: Hub, config: PerigeeConfig) -> Mutex<Self> {
        Mutex::new(Self { verified_blocks: BlockHashSet::new(), first_seen: HashMap::new(), round_start: Instant::now(), config, hub })
    }

    pub fn insert_perigee_timestamp(&mut self, router: &Arc<Router>, hash: Hash, timestamp: Instant, verify: bool) {
        if router.is_perigee() {
            router.add_perigee_timestamp(hash, timestamp);
        }
        if verify {
            self.verify_block(hash);
        }
        self.maybe_insert_first_seen(hash, timestamp);
    }

    pub fn evaluate_round(&self) -> (Vec<PeerKey>, Vec<PeerKey>) {
        let mut peer_table = self.build_table();
        let mut exploitation_peers = Vec::new();
        for _ in (0..self.config.exploitation_target) {
            let top_ranked = match self.get_top_ranked_peer(&peer_table) {
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
            exploitation_peers.push(top_ranked);
            let top_ranked_table = peer_table.remove(&top_ranked).unwrap();
            self.transform_peer_table(&(top_ranked, top_ranked_table), &mut peer_table);
        }

        let remaining_peers = peer_table.into_iter().map(|(k, _)| k).collect::<Vec<PeerKey>>();
        let excused_peers = self.get_excused_peers();

        let eviction_candidates = remaining_peers.into_iter().filter(|p| !excused_peers.contains(p)).collect::<Vec<PeerKey>>();

        if eviction_candidates.len() < self.config.exploration_target {
            // we explore as much as we can.
            (exploitation_peers, eviction_candidates)
        } else {
            // choose peers to evict from perigee at random from the remaining candidates.
            let eviction_candidates =
                eviction_candidates.choose_multiple(&mut rand::thread_rng(), self.config.exploration_target).cloned().collect();
            (exploitation_peers, eviction_candidates)
        }
    }

    pub fn start_new_round(&mut self) {
        self.clear();
        self.round_start = Instant::now();
    }

    pub fn should_evaluate(&mut self) -> bool {
        Instant::now().duration_since(self.round_start).as_secs() > self.config.min_round_duration_in_secs
    }

    pub fn config(&self) -> PerigeeConfig {
        self.config.clone()
    }

    fn maybe_insert_first_seen(&mut self, hash: Hash, timestamp: Instant) {
        match self.first_seen.entry(hash) {
            Entry::Occupied(mut o) => {
                if timestamp < *o.get() {
                    *o.get_mut() = timestamp;
                    debug!("PerigeeManager: Updated first seen timestamp for block {:?} to {:?}", hash, timestamp);
                } else {
                    debug!("PerigeeManager: Existing first seen timestamp for block {:?} is earlier than {:?}, not updating", hash, timestamp);
                }
            }
            Entry::Vacant(v) => {
                v.insert(timestamp);
                debug!("PerigeeManager: Inserted first seen timestamp for block {:?} as {:?}", hash, timestamp);
            }
        }
    }

    fn verify_block(&mut self, hash: Hash) {
        debug!("PerigeeManager: Marking block {:?} as verified", hash);
        self.verified_blocks.insert(hash);
    }

    fn clear(&mut self) {
        debug!["PerigeeManager: Clearing state for new round"];
        self.verified_blocks.clear();
        self.first_seen.clear();
        for router in self.hub.perigee_routers() {
            if router.is_perigee() {
                router.clear_perigee_timestamps();
            }
        }
    }

    fn get_excused_peers(&self) -> Vec<PeerKey> {
        self.hub.perigee_routers().iter().filter(|r| r.connection_started() < self.round_start).map(|r| r.key()).collect()
    }

    fn rate_peer(&self, values: &Vec<u64>) -> u64 {
        let sorted_values = {
            let mut sv = values.clone();
            sv.sort_unstable();
            sv
        };
        return sorted_values[(PERCENTILE_RANK * (sorted_values.len() as f64)).ceil() as usize];
    }

    fn get_top_ranked_peer(&self, peer_table: &HashMap<PeerKey, Vec<u64>>) -> (Option<PeerKey>, u64) {
        debug!("PerigeeManager: Evaluating top ranked peer from table: {:?}", peer_table);
        let mut best_peer: Option<PeerKey> = None;
        let mut best_score = u64::MAX;
        for (peer, delays) in peer_table.iter() {
            let score = self.rate_peer(delays);
            if score < best_score {
                best_score = score;
                best_peer = Some(*peer);
            }
        }
        debug!("PerigeeManager: Top ranked peer is {:?} with score {}", best_peer, best_score);
        return (best_peer, best_score);
    }

    fn transform_peer_table(&self, to_remove: &(PeerKey, Vec<u64>), candidates: &mut HashMap<PeerKey, Vec<u64>>) {
        debug!("PerigeeManager: Transforming peer table by removing peer {:?}", to_remove.0);
        let max_len = candidates.values().map(|v| v.len()).max().unwrap_or(0);

        for j in (0..max_len).rev() {
            let values_at_position: Vec<u64> = candidates.values().map(|vec| vec[j]).collect();
            if values_at_position.iter().all(|&x| x < to_remove.1[j]) {
                for vec in candidates.values_mut() {
                    vec.remove(j);
                }
            }
        }
    }

    fn build_table(&self) -> HashMap<PeerKey, Vec<u64>> {
        debug!("PerigeeManager: Building peer table");
        let mut peer_table: HashMap<PeerKey, Vec<u64>> = HashMap::new();
        for (hash, ts) in self.first_seen.iter() {
            if self.verified_blocks.contains(hash) {
                for peer in self.hub.perigee_routers() {
                    match peer.perigee_timestamps().entry(*hash) {
                        Entry::Occupied(o) => {
                            let delay = o.get().duration_since(*ts).as_millis() as u64;
                            peer_table.entry(peer.key()).or_insert_with(Vec::new).push(delay);
                        }
                        Entry::Vacant(v) => {
                            peer_table.entry(peer.key()).or_insert_with(Vec::new).push(u64::MAX);
                        }
                    }
                }
            };
        }
        peer_table
    }
}
