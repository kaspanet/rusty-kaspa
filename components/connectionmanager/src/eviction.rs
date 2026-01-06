use std::{cmp::Ordering, collections::HashMap};

use itertools::Itertools;
use kaspa_p2p_lib::Peer;
use kaspa_utils::networking::PrefixBucket;
use rand::{seq::SliceRandom, thread_rng, Rng};

use crate::eviction::{cmp_strats::by_lowest_rank, weight_strats::by_highest_none_latency_rank};

/*
# Eviction Logic
This module contains the backbone for the logic of evicting peers from the peer list.
The logic is based on ranking peers, which are calculated based on various metrics.
The ranks are then used to filter and select the peers to be evicted.
The module also contains predefined constants, compare and weight strategy functions to be used in the eviction logic.
*/

/// constants
pub mod constants {
    /// to be used as the ratio for "top performing" lowest ranked peers to retain during eviction.
    pub const RETAIN_RATIO: f64 = 0.4; // inspired from btc
}

/// Predefined compare strategy functions
pub mod cmp_strats {
    use super::EvictionRanks;
    use std::cmp::Ordering;
    /// _**Note**: The lowest rank is the "best" rank, as ranks are organized from low ("good") to high ("bad")._
    #[inline]
    pub fn by_lowest_rank(ranks1: &EvictionRanks, ranks2: &EvictionRanks) -> Ordering {
        ranks1.lowest_rank().total_cmp(&ranks2.lowest_rank())
    }

    #[cfg(test)]
    mod test {
        use super::{by_lowest_rank, EvictionRanks};
        use std::cmp::Ordering;

        #[test]
        fn test_by_lowest_rank() {
            let ranks1 = EvictionRanks {
                last_ping_duration: 0.0,
                time_connected: 3.0,
                last_block_transfer: 3.0,
                last_tx_transfer: 3.0,
                ip_prefix_bucket: 3.0,
            };
            let ranks2 = EvictionRanks {
                last_ping_duration: 1.0,
                time_connected: 2.0,
                last_block_transfer: 2.0,
                last_tx_transfer: 2.0,
                ip_prefix_bucket: 2.0,
            };
            assert_eq!(by_lowest_rank(&ranks1, &ranks2), Ordering::Less);
            assert_eq!(by_lowest_rank(&ranks2, &ranks1), Ordering::Greater);

            let ranks1 = EvictionRanks {
                last_ping_duration: 0.0,
                time_connected: 0.0,
                last_block_transfer: 0.0,
                last_tx_transfer: 0.0,
                ip_prefix_bucket: 0.0,
            };
            let ranks2 = EvictionRanks {
                last_ping_duration: 0.0,
                time_connected: 0.0,
                last_block_transfer: 0.0,
                last_tx_transfer: 0.0,
                ip_prefix_bucket: 0.0,
            };
            assert_eq!(by_lowest_rank(&ranks1, &ranks2), Ordering::Equal);
        }
    }
}

/// Predefined weight strategy functions
pub mod weight_strats {
    use super::EvictionRanks;
    /// _**Note**: The highest rank is the "worst" rank, as ranks are organized from low ("good") to high ("bad")._
    #[inline]
    pub fn by_highest_none_latency_rank(ranks: &EvictionRanks) -> f64 {
        ranks.highest_none_latency_rank() + 1.0 // add 1 to avoid 0 weight
    }

    #[cfg(test)]
    mod test {
        use super::{by_highest_none_latency_rank, EvictionRanks};

        #[test]
        fn test_by_highest_none_latency_rank() {
            let ranks = EvictionRanks {
                last_ping_duration: 0.0,
                time_connected: 2.0,
                last_block_transfer: 0.0,
                last_tx_transfer: 0.0,
                ip_prefix_bucket: 1.0,
            };
            assert_eq!(by_highest_none_latency_rank(&ranks), 3.0);

            let ranks = EvictionRanks {
                last_ping_duration: 0.0,
                time_connected: 1.0,
                last_block_transfer: 0.0,
                last_tx_transfer: 0.0,
                ip_prefix_bucket: 2.0,
            };
            assert_eq!(by_highest_none_latency_rank(&ranks), 3.0);

            let ranks = EvictionRanks {
                last_ping_duration: 1.0,
                time_connected: 0.0,
                last_block_transfer: 1.0,
                last_tx_transfer: 1.0,
                ip_prefix_bucket: 0.0,
            };

            assert_eq!(by_highest_none_latency_rank(&ranks), 1.0);

            let ranks = EvictionRanks {
                last_ping_duration: 0.0,
                time_connected: 1.0,
                last_block_transfer: 0.0,
                last_tx_transfer: 0.0,
                ip_prefix_bucket: 1.0,
            };

            assert_eq!(by_highest_none_latency_rank(&ranks), 2.0);
        }
    }
}

/// Holds Ranks used for eviction logic
///
/// _**Note**:_
///
/// _1) Ranks are organized from low to high, the lower the rank, the better the peer's perf in that metric._
///
/// _2) A peer may hold a rank as a multiple of 0.5, due to tie breaks splitting the rank._
#[derive(Default, Clone, Copy, PartialEq, Debug)]
pub struct EvictionRanks {
    ip_prefix_bucket: f64, // the first byte of the IP address, used to group peers by their IP prefix, low value indicates a peer from a less populated prefix.
    time_connected: f64,   // ranked time connected, low value indicates a peer with a long consistent connection to us.
    last_ping_duration: f64, // ranked last ping duration, low value indicates a peer with a low latency.
    last_block_transfer: f64, // ranked last block transfer duration, low value indicates a peer that is actively sending blocks to us.
    last_tx_transfer: f64, // ranked last transaction transfer duration, low value indicates a peer that is actively sending transactions to us.
}

impl EvictionRanks {
    /// Returns and defines an array of all ranks that are not latency based.
    ///
    /// _**Includes:** [`Self::time_connected`], and [`Self::ip_prefix_bucket`]._
    ///
    /// _**Excludes:** [`Self::last_ping_duration`], [`Self::last_block_transfer`], and [`Self::last_tx_transfer`]._
    #[inline]
    fn none_latency_ranks(&self) -> [f64; 2] {
        [self.time_connected, self.ip_prefix_bucket]
    }

    /// Returns and defines an array of all ranks in the struct.
    ///
    /// **Includes:** [`Self::last_ping_duration`], [`Self::time_connected`], [`Self::last_block_transfer`], [`Self::last_tx_transfer`], and [`Self::ip_prefix_bucket`].
    #[inline]
    fn all_ranks(&self) -> [f64; 5] {
        [self.last_ping_duration, self.time_connected, self.last_block_transfer, self.last_tx_transfer, self.ip_prefix_bucket]
    }

    /// Returns the lowest rank of all ranks in the struct.
    ///
    /// Refer to [`Self::all_ranks`] for the ranks.
    ///
    /// _**Note**: The lowest rank is the "best" rank, as ranks are organized from low ("good") to high ("bad")._
    ///
    /// _**Includes:** [`Self::last_ping_duration`], [`Self::time_connected`], [`Self::last_block_transfer`], [`Self::last_tx_transfer`], and [`Self::ip_prefix_bucket`]._
    #[inline]
    pub fn lowest_rank(&self) -> f64 {
        self.all_ranks().into_iter().reduce(move |a, b| a.min(b)).expect("expected at least one rank")
    }

    /// Returns the highest rank of the non-latency ranks / variables.
    ///
    /// _**Note**: The highest rank is the "worst" rank, as ranks are organized from low ("good") to high ("bad")._
    ///
    /// _**Includes:** [`Self::time_connected`], and [`Self::ip_prefix_bucket`]._
    ///
    /// _**Excludes:** [`Self::last_ping_duration`], [`Self::last_block_transfer`], and [`Self::last_tx_transfer`]._
    #[inline]
    pub fn highest_none_latency_rank(&self) -> f64 {
        self.none_latency_ranks().into_iter().reduce(move |a, b| a.max(b)).expect("expected at least one rank")
    }
}

pub trait EvictionIterExt<'a, Iter>: IntoIterator<Item = (&'a Peer, EvictionRanks), IntoIter = Iter>
where
    Iter: Iterator<Item = (&'a Peer, EvictionRanks)> + 'a,
{
    fn retain_lowest_rank_peers(self, amount: usize) -> impl Iterator<Item = (&'a Peer, EvictionRanks)> + 'a
    where
        Self: Sized,
    {
        let rng = &mut thread_rng();
        self.into_iter()
            .sorted_unstable_by(move |(_, r1), (_, r2)| match by_lowest_rank(r1, r2) {
                Ordering::Greater => Ordering::Greater,
                Ordering::Less => Ordering::Less,
                // we tie break randomly, as to not expose preference due to pre-existing ordering.
                Ordering::Equal => {
                    if rng.gen_bool(0.5) {
                        Ordering::Greater
                    } else {
                        Ordering::Less
                    }
                }
            })
            .skip(amount)
    }

    fn evict_by_highest_none_latency_rank_weighted(self, amount: usize) -> impl Iterator<Item = (&'a Peer, EvictionRanks)> + 'a
    where
        Self: Sized,
    {
        let rng = &mut thread_rng();
        self.into_iter()
            .collect_vec()
            .choose_multiple_weighted(rng, amount, |(_, r)| by_highest_none_latency_rank(r))
            .unwrap()
            .copied()
            .collect_vec()
            .into_iter()
    }

    fn iterate_peers(self) -> impl Iterator<Item = &'a Peer> + 'a
    where
        Self: Sized,
    {
        self.into_iter().map(|(p, _)| p)
    }
}

impl<'a, Iter, IntoIter> EvictionIterExt<'a, Iter> for IntoIter
where
    Iter: Iterator<Item = (&'a Peer, EvictionRanks)> + 'a,
    IntoIter: IntoIterator<Item = (&'a Peer, EvictionRanks), IntoIter = Iter>,
{
}

pub fn eviction_iter_from_peers<'a>(peers: &'a [&'a Peer]) -> impl Iterator<Item = (&'a Peer, EvictionRanks)> + 'a {
    let ip_prefix_histogram = build_ip_prefix_histogram(peers);
    let mut ranks = vec![EvictionRanks::default(); peers.len()];
    peers.iter().enumerate().map(move |(i1, p1)| {
        for (i2, p2) in peers[i1..].iter().enumerate().skip(1) {
            match ip_prefix_histogram[&p1.prefix_bucket()].cmp(&ip_prefix_histogram[&p2.prefix_bucket()]) {
                // low is good, so we add rank to the peer with the greater ordering.
                Ordering::Greater => ranks[i1].ip_prefix_bucket += 1.0,
                Ordering::Less => ranks[i1 + i2].ip_prefix_bucket += 1.0,
                Ordering::Equal => {
                    ranks[i1].ip_prefix_bucket += 0.5;
                    ranks[i1 + i2].ip_prefix_bucket += 0.5;
                }
            };

            match p1.time_connected().cmp(&p2.time_connected()) {
                // high is good, so we add to the peer with the lesser ordering.
                Ordering::Greater => ranks[i1 + i2].time_connected += 1.0,
                Ordering::Less => ranks[i1].time_connected += 1.0,
                Ordering::Equal => {
                    ranks[i1].time_connected += 0.5;
                    ranks[i1 + i2].time_connected += 0.5;
                }
            };

            match p1.last_ping_duration().cmp(&p2.last_ping_duration()) {
                // low is good, so we add to the peer with the greater ordering.
                Ordering::Greater => ranks[i1].last_ping_duration += 1.0,
                Ordering::Less => ranks[i1 + i2].last_ping_duration += 1.0,
                Ordering::Equal => {
                    ranks[i1].last_ping_duration += 0.5;
                    ranks[i1 + i2].last_ping_duration += 0.5;
                }
            };

            match (p1.last_block_transfer(), p2.last_block_transfer()) {
                // Some is good, so we add to the peer with None
                (Some(_), None) => ranks[i1 + i2].last_block_transfer += 1.0,
                (None, Some(_)) => ranks[i1].last_block_transfer += 1.0,
                (None, None) => {
                    ranks[i1].last_block_transfer += 0.5;
                    ranks[i1 + i2].last_block_transfer += 0.5;
                }
                (Some(peer1_last_block_transfer), Some(peer2_last_block_transfer)) => {
                    match peer1_last_block_transfer.cmp(&peer2_last_block_transfer) {
                        // low is good, so we add to the peer with the greater ordering.
                        Ordering::Greater => ranks[i1].last_block_transfer += 1.0,
                        Ordering::Less => ranks[i1 + i2].last_block_transfer += 1.0,
                        Ordering::Equal => {
                            ranks[i1].last_block_transfer += 0.5;
                            ranks[i1 + i2].last_block_transfer += 0.5;
                        }
                    }
                }
            };

            match (p1.last_tx_transfer(), p2.last_tx_transfer()) {
                // Some is good, so we add to the peer with None
                (Some(_), None) => ranks[i1 + i2].last_tx_transfer += 1.0,
                (None, Some(_)) => ranks[i1].last_tx_transfer += 1.0,
                (None, None) => {
                    ranks[i1].last_tx_transfer += 0.5;
                    ranks[i1 + i2].last_tx_transfer += 0.5;
                }
                (Some(peer1_last_tx_transfer), Some(peer2_last_tx_transfer)) => {
                    match peer1_last_tx_transfer.cmp(&peer2_last_tx_transfer) {
                        // low is good, so we add to the peer with the greater ordering.
                        Ordering::Greater => ranks[i1].last_tx_transfer += 1.0,
                        Ordering::Less => ranks[i1 + i2].last_tx_transfer += 1.0,
                        Ordering::Equal => {
                            ranks[i1].last_tx_transfer += 0.5;
                            ranks[i1 + i2].last_tx_transfer += 0.5;
                        }
                    }
                }
            };
        }
        (peers[i1], ranks[i1])
    })
}
// Abstracted helper functions:

fn build_ip_prefix_histogram(peers: &[&Peer]) -> HashMap<PrefixBucket, usize> {
    let mut ip_prefix_histogram = HashMap::new();
    for peer in peers.iter() {
        *ip_prefix_histogram.entry(peer.prefix_bucket()).or_insert(1) += 1;
    }
    ip_prefix_histogram
}

#[cfg(test)]
mod test {

    use super::*;

    use kaspa_core::{debug, info, log::try_init_logger};
    use kaspa_p2p_lib::Peer;
    use kaspa_utils::networking::PeerId;
    use std::net::SocketAddr;
    use std::{
        net::SocketAddrV4,
        str::FromStr,
        time::{Duration, Instant},
    };
    use uuid::Uuid;

    fn build_test_peers() -> Vec<Peer> {
        let now = Instant::now();
        let instants = Vec::from_iter((0..10).map(|i| now.checked_sub(Duration::from_secs(i)).unwrap())); // `from_secs` is important here, as time_connected has ms granularity, so it most be greater granularity than ms.
                                                                                                          // rank 0, 1, 2, 3, 5, => 0, 1
        let peer0 = Peer::new(
            PeerId::from(Uuid::from_u128(0u128)),
            SocketAddr::V4(SocketAddrV4::from_str("1.0.0.0:1").unwrap()),
            Default::default(),
            instants[8],
            Default::default(),
            2,
            Some(instants[4]),
            Some(instants[3]),
        );
        // rank 1.5, 0, 1, 2, 3 => 0, 1.5
        let peer1 = Peer::new(
            PeerId::from(Uuid::from_u128(1u128)),
            SocketAddr::V4(SocketAddrV4::from_str("2.0.0.0:1").unwrap()),
            Default::default(),
            instants[9],
            Default::default(),
            1,
            Some(instants[5]),
            Some(instants[4]),
        );
        // rank 1.5, 2, 0, 1, 2 => 0, 2
        let peer2 = Peer::new(
            PeerId::from(Uuid::from_u128(2u128)),
            SocketAddr::V4(SocketAddrV4::from_str("2.0.0.0:1").unwrap()),
            Default::default(),
            instants[7],
            Default::default(),
            0,
            Some(instants[6]),
            Some(instants[5]),
        );
        // rank 4, 3, 3, 0, 1 => 0, 4
        let peer3 = Peer::new(
            PeerId::from(Uuid::from_u128(3u128)),
            SocketAddr::V4(SocketAddrV4::from_str("3.0.0.0:1").unwrap()),
            Default::default(),
            instants[6],
            Default::default(),
            3,
            Some(instants[7]),
            Some(instants[6]),
        );
        // rank 4, 4, 4, 4, 0 => 0, 4
        let peer4 = Peer::new(
            PeerId::from(Uuid::from_u128(4u128)),
            SocketAddr::V4(SocketAddrV4::from_str("3.0.0.0:1").unwrap()),
            Default::default(),
            instants[5],
            Default::default(),
            4,
            Some(instants[3]),
            Some(instants[7]),
        );
        // rank 4, 5, 5, 5, 5 => 4, 5
        let peer5 = Peer::new(
            PeerId::from(Uuid::from_u128(5u128)),
            SocketAddr::V4(SocketAddrV4::from_str("3.0.0.0:1").unwrap()),
            Default::default(),
            instants[4],
            Default::default(),
            5,
            Some(instants[2]),
            Some(instants[2]),
        );
        // rank 7.5, 6, 6, 6, 8.5 => 6, 7.5
        let peer6 = Peer::new(
            PeerId::from(Uuid::from_u128(6u128)),
            SocketAddr::V4(SocketAddrV4::from_str("4.0.0.0:1").unwrap()),
            Default::default(),
            instants[3],
            Default::default(),
            6,
            Some(instants[1]),
            None,
        );
        // rank 7.5, 7, 7, 7, 8.5 => 7, 7.5
        let peer7 = Peer::new(
            PeerId::from(Uuid::from_u128(7u128)),
            SocketAddr::V4(SocketAddrV4::from_str("4.0.0.0:1").unwrap()),
            Default::default(),
            instants[2],
            Default::default(),
            7,
            Some(instants[0]),
            None,
        );
        // rank 7.5, 8, 8, 8.5, 6 => 6, 8
        let peer8 = Peer::new(
            PeerId::from(Uuid::from_u128(8u128)),
            SocketAddr::V4(SocketAddrV4::from_str("4.0.0.0:1").unwrap()),
            Default::default(),
            instants[1],
            Default::default(),
            8,
            None,
            Some(instants[1]),
        );
        // rank 7.5, 9, 9, 8.5, 7 => 7, 9
        let peer9 = Peer::new(
            PeerId::from(Uuid::from_u128(9u128)),
            SocketAddr::V4(SocketAddrV4::from_str("4.0.0.0:1").unwrap()),
            Default::default(),
            instants[0],
            Default::default(),
            9,
            None,
            Some(instants[0]),
        );

        vec![peer0, peer1, peer2, peer3, peer4, peer5, peer6, peer7, peer8, peer9]
    }

    #[test]
    fn test_eviction_ranks() {
        let ranks = EvictionRanks {
            ip_prefix_bucket: 4.0,
            time_connected: 1.0,
            last_ping_duration: 0.0,
            last_block_transfer: 2.0,
            last_tx_transfer: 3.0,
        };
        assert_eq!(ranks.lowest_rank(), 0.0);
        assert_eq!(ranks.highest_none_latency_rank(), 4.0);

        let ranks = EvictionRanks {
            ip_prefix_bucket: 3.0,
            time_connected: 0.0,
            last_ping_duration: 4.0,
            last_block_transfer: 1.0,
            last_tx_transfer: 2.0,
        };
        assert_eq!(ranks.lowest_rank(), 0.0);
        assert_eq!(ranks.highest_none_latency_rank(), 3.0);

        let ranks = EvictionRanks {
            ip_prefix_bucket: 2.0,
            time_connected: 4.0,
            last_ping_duration: 3.0,
            last_block_transfer: 0.0,
            last_tx_transfer: 1.0,
        };
        assert_eq!(ranks.lowest_rank(), 0.0);
        assert_eq!(ranks.highest_none_latency_rank(), 4.0);

        let ranks = EvictionRanks {
            ip_prefix_bucket: 1.0,
            time_connected: 3.0,
            last_ping_duration: 2.0,
            last_block_transfer: 4.0,
            last_tx_transfer: 0.0,
        };
        assert_eq!(ranks.lowest_rank(), 0.0);
        assert_eq!(ranks.highest_none_latency_rank(), 3.0);

        let ranks = EvictionRanks {
            ip_prefix_bucket: 0.0,
            time_connected: 2.0,
            last_ping_duration: 1.0,
            last_block_transfer: 3.0,
            last_tx_transfer: 4.0,
        };
        assert_eq!(ranks.lowest_rank(), 0.0);
        assert_eq!(ranks.highest_none_latency_rank(), 2.0);
    }

    #[test]
    fn test_eviction_iter_from_peers() {
        let test_peers = build_test_peers();
        let test_peers = test_peers.iter().collect::<Vec<_>>();
        let eviction_iter_vec = eviction_iter_from_peers(&test_peers).collect::<Vec<(&Peer, EvictionRanks)>>();

        let expected_ranks = vec![
            EvictionRanks {
                ip_prefix_bucket: 0.0,
                time_connected: 1.0,
                last_ping_duration: 2.0,
                last_block_transfer: 3.0,
                last_tx_transfer: 4.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 1.5,
                time_connected: 0.0,
                last_ping_duration: 1.0,
                last_block_transfer: 2.0,
                last_tx_transfer: 3.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 1.5,
                time_connected: 2.0,
                last_ping_duration: 0.0,
                last_block_transfer: 1.0,
                last_tx_transfer: 2.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 4.0,
                time_connected: 3.0,
                last_ping_duration: 3.0,
                last_block_transfer: 0.0,
                last_tx_transfer: 1.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 4.0,
                time_connected: 4.0,
                last_ping_duration: 4.0,
                last_block_transfer: 4.0,
                last_tx_transfer: 0.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 4.0,
                time_connected: 5.0,
                last_ping_duration: 5.0,
                last_block_transfer: 5.0,
                last_tx_transfer: 5.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 7.5,
                time_connected: 6.0,
                last_ping_duration: 6.0,
                last_block_transfer: 6.0,
                last_tx_transfer: 8.5,
            },
            EvictionRanks {
                ip_prefix_bucket: 7.5,
                time_connected: 7.0,
                last_ping_duration: 7.0,
                last_block_transfer: 7.0,
                last_tx_transfer: 8.5,
            },
            EvictionRanks {
                ip_prefix_bucket: 7.5,
                time_connected: 8.0,
                last_ping_duration: 8.0,
                last_block_transfer: 8.5,
                last_tx_transfer: 6.0,
            },
            EvictionRanks {
                ip_prefix_bucket: 7.5,
                time_connected: 9.0,
                last_ping_duration: 9.0,
                last_block_transfer: 8.5,
                last_tx_transfer: 7.0,
            },
        ];

        assert_eq!(eviction_iter_vec.len(), expected_ranks.len());
        for (i, (_, ranks)) in eviction_iter_vec.iter().enumerate() {
            assert_eq!(ranks, &expected_ranks[i]);
        }
    }

    #[test]
    fn test_eviction_iter_filter_peers() {
        let test_peers = build_test_peers();
        let test_peers = test_peers.iter().collect::<Vec<_>>();
        let iterations = test_peers.len();
        let eviction_iter_vec = eviction_iter_from_peers(&test_peers).collect::<Vec<(&Peer, EvictionRanks)>>();

        for i in 0..iterations + 1 {
            let mut removed_counter = HashMap::<u64, usize>::new();
            let mut filtered_counter = HashMap::<u64, usize>::new();
            let eviction_candidates_iter = eviction_iter_vec.clone().into_iter();
            let filtered_eviction_set = eviction_candidates_iter.retain_lowest_rank_peers(i).collect_vec();
            let removed_eviction_set = eviction_iter_vec
                .clone()
                .into_iter()
                .filter(|item| !filtered_eviction_set.iter().any(|&x| x.0.identity() == item.0.identity()))
                .collect_vec();
            assert_eq!(filtered_eviction_set.len(), iterations - i);
            assert_eq!(removed_eviction_set.len(), i);
            for (_, er) in &removed_eviction_set {
                *removed_counter.entry(er.lowest_rank().to_bits()).or_insert(0) += 1;
            }
            for (_, er) in &filtered_eviction_set {
                *filtered_counter.entry(er.lowest_rank().to_bits()).or_insert(0) += 1;
            }
            match i {
                0 => {
                    assert_eq!(removed_counter.len(), 0);

                    assert_eq!(filtered_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(filtered_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 4);
                }
                1 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter.len(), 1);

                    assert_eq!(filtered_counter[&0.0_f64.to_bits()], 4);
                    assert_eq!(filtered_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 4);
                }
                2 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 2);
                    assert_eq!(removed_counter.len(), 1);

                    assert_eq!(filtered_counter[&0.0_f64.to_bits()], 3);
                    assert_eq!(filtered_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 4);
                }
                3 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 3);
                    assert_eq!(removed_counter.len(), 1);

                    assert_eq!(filtered_counter[&0.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 4);
                }
                4 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 4);
                    assert_eq!(removed_counter.len(), 1);

                    assert_eq!(filtered_counter[&0.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 4);
                }
                5 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(removed_counter.len(), 1);

                    assert_eq!(filtered_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 3);
                }
                6 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(removed_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter.len(), 2);

                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 2);
                }
                7 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(removed_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter[&6.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter.len(), 3);

                    assert_eq!(filtered_counter[&6.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 2);
                }
                8 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(removed_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(removed_counter.len(), 3);

                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(filtered_counter.len(), 1);
                }
                9 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(removed_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(removed_counter[&7.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter.len(), 4);

                    assert_eq!(filtered_counter[&7.0_f64.to_bits()], 1);
                    assert_eq!(filtered_counter.len(), 1);
                }
                10 => {
                    assert_eq!(removed_counter[&0.0_f64.to_bits()], 5);
                    assert_eq!(removed_counter[&4.0_f64.to_bits()], 1);
                    assert_eq!(removed_counter[&6.0_f64.to_bits()], 2);
                    assert_eq!(removed_counter[&7.0_f64.to_bits()], 2);
                    assert_eq!(removed_counter.len(), 4);

                    assert_eq!(filtered_counter.len(), 0);
                }
                _ => panic!("unexpected i value"),
            }
        }
    }

    #[test]
    fn test_eviction_iter_select_peers_weighted() {
        try_init_logger("info");
        let test_peers = build_test_peers();
        let test_peers = test_peers.iter().collect::<Vec<_>>();
        let eviction_iter_vec = eviction_iter_from_peers(&test_peers).collect::<Vec<(&Peer, EvictionRanks)>>();
        let total_weight = 59.5;
        let expected_probabilities = vec![
            // we add one to avoid 0, and nan numbers.. `weight_strats::by_highest_none_latency_rank` adds 1 to the rank for these hypothetical situation.
            (1.0 + 1.0) / total_weight,
            (1.5 + 1.0) / total_weight,
            (2.0 + 1.0) / total_weight,
            (4.0 + 1.0) / total_weight * 2.0, // we have two 4.0 ranks
            (5.0 + 1.0) / total_weight,
            (7.5 + 1.0) / total_weight * 2.0, // we have two 7.5 ranks
            (8.0 + 1.0) / total_weight,
            (9.0 + 1.0) / total_weight,
        ];
        assert_eq!(expected_probabilities.iter().sum::<f64>(), 1.0);
        assert_eq!(expected_probabilities.len(), 8);

        let mut selected_counter = HashMap::<u64, f64>::new();
        let num_of_trials = 2054;
        for _ in 0..num_of_trials {
            //println!("sample_size: {}", sample_size);
            let eviction_iter = eviction_iter_vec.clone().into_iter();
            let selected_eviction_set = eviction_iter.evict_by_highest_none_latency_rank_weighted(1).collect_vec();
            assert_eq!(selected_eviction_set.len(), 1);
            for (_, er) in &selected_eviction_set {
                let highest_none_latency_rank = er.highest_none_latency_rank();
                *selected_counter.entry(highest_none_latency_rank.to_bits()).or_insert(0.0) += 1.0;
            }
        }
        let mut actual_probabilities = vec![0.0; expected_probabilities.len()];
        for (rank, count) in selected_counter {
            let rank = f64::from_bits(rank);
            if rank == 1.0 {
                actual_probabilities[0] = count / num_of_trials as f64;
            } else if rank == 1.5 {
                actual_probabilities[1] = count / num_of_trials as f64;
            } else if rank == 2.0 {
                actual_probabilities[2] = count / num_of_trials as f64;
            } else if rank == 4.0 {
                actual_probabilities[3] = count / num_of_trials as f64;
            } else if rank == 5.0 {
                actual_probabilities[4] = count / num_of_trials as f64;
            } else if rank == 7.5 {
                actual_probabilities[5] = count / num_of_trials as f64;
            } else if rank == 8.0 {
                actual_probabilities[6] = count / num_of_trials as f64;
            } else if rank == 9.0 {
                actual_probabilities[7] = count / num_of_trials as f64;
            } else {
                panic!("unexpected rank value: {}", rank);
            }
        }
        debug!("expected_probabilities: \n {:?}", expected_probabilities);
        debug!("actual_probabilities: \n {:?}", actual_probabilities);
        let p = 1.0
            - rv::misc::ks_two_sample(
                &expected_probabilities,
                &actual_probabilities,
                rv::misc::KsMode::Exact,
                rv::misc::KsAlternative::TwoSided,
            )
            .unwrap()
            .1;
        assert!(p < 0.05);
        info!("Kolmogorov–Smirnov test result for `EvictionIter.select_peers_weighted`: p = {0:.3}, p < 0.05", p);
    }
}
