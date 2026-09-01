use std::sync::Arc;

use kaspa_consensus_core::{BlueWorkType, KType};
use kaspa_hashes::Hash;
use kaspa_math::int::SignedInteger;

use crate::model::stores::ghostdag::GhostdagData;

/// Signed blue work — `SignedInteger<BlueWorkType>` with sign, used for cascade score
/// accumulation (Uint192-safe).
pub type SignedWork = SignedInteger<BlueWorkType>;

/// Read-only access to the k-coloring (`GhostdagData`) of conflict-zone blocks.
/// Implemented by `ConflictZoneManager`; keeps UMC voters decoupled from the full
/// manager so they can be unit-tested with synthetic coloring chains.
pub trait ColoringReader {
    /// Returns the stored coloring data for a zone block.
    ///
    /// Zone data for every chain block between the virtual GD and the conflict genesis
    /// is guaranteed present by `fill_zone_data` + `k_colouring`; a missing entry is a bug.
    fn get_coloring_data(&self, hash: Hash) -> Arc<GhostdagData>;
}

/// Input to a single UMC voting call, shared by all `UmcVoter` implementations.
pub struct UmcVotingContext<'a> {
    /// The latest common chain ancestor of the zone (conflict genesis).
    pub conflict_genesis: Hash,
    /// The next chain ancestor of the subgroup being evaluated above the conflict genesis
    pub next_chain_ancestor: &'a Hash,
    /// The k-coloring data of the virtual GD — head of the virtual GD chain.
    pub virtual_gd: &'a GhostdagData,
    /// The rank `k` under test.
    pub k: KType,
    /// Read access to the virtual GD chain below `virtual_gd` towards `conflict_genesis`.
    pub coloring_reader: &'a dyn ColoringReader,
}

/// Trait for UMC cascade voting, isolating the voting strategy from the DAGKnight
/// executor for testability. Mirrors the `TieBreaker` trait in `tie_breaking.rs`.
pub trait UmcVoter {
    /// Runs UMC voting for the zone described by `ctx`.
    fn vote(&self, ctx: &UmcVotingContext<'_>) -> CascadeResult;
}

/// Cascade result including flip statistics for performance monitoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadeResult {
    pub virtual_score: SignedWork,
    pub accepted: bool,
    pub flips: u64,
    pub voting_blocks: u64,
    /// Whether this cascade started from a persisted checkpoint state.
    pub from_checkpoint: bool,
    /// Estimated blue blocks skipped by loading from checkpoint.
    /// Calculated as virtual_gd.blue_score - checkpoint_block.blue_score,
    /// representing the number of blue blocks we didn't need to visit.
    /// Zero if cascade started from scratch.
    pub estimated_effort_saved: u64,
    /// Total blue blocks in the conflict zone (virtual_gd.blue_score).
    /// Used as denominator for effort_saved percentage.
    pub estimated_effort_total: u64,
}

#[cfg(test)]
#[allow(clippy::arc_with_non_send_sync)]
pub mod test_fixtures {
    use std::{collections::HashMap, sync::Arc};

    use kaspa_consensus_core::{
        BlockHashMap, HashKTypeMap, HashMapCustomHasher, KType,
        blockhash::{BlockHashes, ORIGIN},
        header::Header,
    };
    use kaspa_hashes::Hash;
    use parking_lot::RwLock;
    use serde::Deserialize;

    use super::{SignedWork, UmcVotingContext};
    use crate::processes::{
        difficulty::calc_work,
        reachability::tests::{DagBlock, DagBuilder},
    };
    use crate::{
        model::{
            services::reachability::{MTReachabilityService, ReachabilityService},
            stores::{
                ghostdag::GhostdagData, headers::MemoryHeaderStore, reachability::MemoryReachabilityStore,
                relations::MemoryRelationsStore,
            },
        },
        processes::dagknight::umc_voting::ColoringReader,
    };

    /// In-memory `ColoringReader` backed by a map of synthetic `GhostdagData` per chain block.
    #[derive(Default)]
    pub struct MemoryColoringReader {
        by_block: HashMap<Hash, Arc<GhostdagData>>,
    }

    impl MemoryColoringReader {
        pub fn add(&mut self, hash: Hash, gd: GhostdagData) {
            self.by_block.insert(hash, Arc::new(gd));
        }
    }

    impl ColoringReader for MemoryColoringReader {
        fn get_coloring_data(&self, hash: Hash) -> Arc<GhostdagData> {
            self.by_block.get(&hash).cloned().expect("memory coloring reader: block not in zone")
        }
    }

    pub fn make_gd(sp: Hash, blues: Vec<Hash>, reds: Vec<Hash>, blue_score: u64) -> GhostdagData {
        GhostdagData::new(
            blue_score,
            Default::default(),
            sp,
            BlockHashes::new(blues),
            BlockHashes::new(reds),
            HashKTypeMap::new(BlockHashMap::new()),
        )
    }

    /// JSON shape of the UMC fixture zone in umc_fixture.json
    #[derive(Deserialize)]
    struct UmcFixture {
        genesis: u64,
        k: KType,
        subgroup: Vec<u64>,
        blocks: Vec<UmcFixtureBlock>,
        #[serde(rename = "virtual")]
        virtual_gd: VirtualGd,
    }

    #[derive(Deserialize)]
    struct UmcFixtureBlock {
        id: u64,
        parents: Vec<u64>,
        sp: u64,
        blue_work: u64,
        #[serde(default)]
        gd: Option<ZoneGd>,
    }

    /// A zone block's coloring mergeset; its selected parent is the block's own `sp`.
    #[derive(Deserialize)]
    struct ZoneGd {
        blues: Vec<u64>,
        reds: Vec<u64>,
        blue_score: u64,
    }

    #[derive(Deserialize)]
    struct VirtualGd {
        sp: u64,
        blues: Vec<u64>,
        reds: Vec<u64>,
        blue_score: u64,
    }

    /// UMC voting fixture with a known and hand-calculated result. The zone data (topology,
    /// selected parents, header blue works, and the zone coloring) is loaded from
    /// `dag_knight_umc_fixture.json`; it is the whitepaper DAG's final conflict (tips
    /// `[11, 17]`, conflict genesis `1`, subgroup `[11]`, `k = 0`), where `8` is the one gray.
    /// See `expected_score()` for the protocol-independent oracle of the expected score.
    pub struct Fixture {
        pub reachability: MTReachabilityService<MemoryReachabilityStore>,
        pub headers: Arc<MemoryHeaderStore>,
        pub reader: MemoryColoringReader,
        pub virtual_gd: Arc<GhostdagData>,
        pub next_chain_ancestor: Hash,
        pub conflict_genesis: Hash,
        pub k: KType,
    }

    impl Default for Fixture {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Fixture {
        pub fn new() -> Self {
            let path = format!("{}/umc_fixture.json", env!("CARGO_MANIFEST_DIR"));
            let file = std::fs::File::open(&path).expect("Unable to open UMC fixture JSON");
            let data: UmcFixture = serde_json::from_reader(file).expect("Unable to parse UMC fixture JSON");

            let block_work = calc_work(0x207fffff);
            let mut reachability = MemoryReachabilityStore::new();
            let mut relations = MemoryRelationsStore::new();
            let mut builder = DagBuilder::new(&mut reachability, &mut relations);
            builder.init();

            let headers = Arc::new(MemoryHeaderStore::new());
            let mut reader = MemoryColoringReader::default();

            for block in &data.blocks {
                let hash: Hash = block.id.into();
                let parents: Vec<Hash> = block.parents.iter().map(|&p| p.into()).collect();
                let sp: Hash = block.sp.into();

                if block.id == data.genesis {
                    builder.add_block(DagBlock::new(hash, vec![ORIGIN]));
                } else {
                    builder.add_block_with_selected_parent(DagBlock::new(hash, parents.clone()), sp);
                }

                // Header with uniform bits; cumulative blue work = blue_work * per-block work.
                let mut header = Header::from_precomputed_hash(hash, parents);
                header.bits = 0x207fffff;
                header.blue_work = block_work * block.blue_work;
                headers.insert(Arc::new(header));

                // Zone coloring entry (only blocks the voter reads carry one).
                if let Some(gd) = &block.gd {
                    let blues: Vec<Hash> = gd.blues.iter().map(|&b| b.into()).collect();
                    let reds: Vec<Hash> = gd.reds.iter().map(|&r| r.into()).collect();
                    reader.add(hash, make_gd(sp, blues, reds, gd.blue_score));
                }
            }

            let virtual_gd = Arc::new(make_gd(
                data.virtual_gd.sp.into(),
                data.virtual_gd.blues.iter().map(|&b| b.into()).collect(),
                data.virtual_gd.reds.iter().map(|&r| r.into()).collect(),
                data.virtual_gd.blue_score,
            ));

            let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability)));
            let next_chain_ancestor = reachability_service.get_next_chain_ancestor(data.subgroup[0].into(), data.genesis.into());
            Self {
                reachability: reachability_service,
                headers,
                reader,
                virtual_gd,
                next_chain_ancestor,
                conflict_genesis: data.genesis.into(),
                k: data.k,
            }
        }

        pub fn context(&self) -> UmcVotingContext<'_> {
            UmcVotingContext {
                conflict_genesis: self.conflict_genesis,
                next_chain_ancestor: &self.next_chain_ancestor,
                virtual_gd: &self.virtual_gd,
                k: self.k,
                coloring_reader: &self.reader,
            }
        }

        /// Expected `virtual_score`, hand-calculated from the zone (loaded from
        /// `umc_fixture.json`) as a protocol-independent oracle (the voters must agree
        /// with this without being able to read it):
        ///
        ///   virtual_score = Σ_blue vote(B) + deficit − Σ_red work(R)
        ///
        /// Voting blues: 11, 10, 9, 7, 6, 5, 4, 3, 2 + CG(1);
        /// Voting reds: 12..17
        /// Gray = 8 (does not vote)
        ///
        ///   vote(11) = +w  future blues {},                        0 − 0 = 0 ≥ 0
        ///   vote(10) = +w  future blues {11} = +1w,                1w ≥ 0
        ///   vote(9)  = +w  future blues {10, 11} = +2w,            2w ≥ 0
        ///   vote(7)  = +w  future blues {9, 10, 11} = +3w,         3w ≥ 0
        ///   vote(6)  = +w  future blues {7, 9, 10, 11} = +4w, reds {17} = 1w,  4w − 1w ≥ 0
        ///   vote(5)  = +w  future blues +5w, reds {17} = 1w,       5w − 1w ≥ 0
        ///   vote(4)  = +w  future blues +6w, reds {17} = 1w,       6w − 1w ≥ 0
        ///   vote(3)  = +w  future blues +7w, reds {17} = 1w,       7w − 1w ≥ 0
        ///   vote(2)  = +w  future blues +8w, reds {17} = 1w,       8w − 1w ≥ 0
        ///   vote(1)  = +w  future blues +9w, reds {12..17} = 6w,   9w − 6w ≥ 0
        ///
        /// (Gray red 8 never votes although it is a red in the futures of 2 (the NCA) — the NCA
        /// filter removes it. Reds come from blocks extending NCA=12
        ///
        /// virtual_score = (10 × +w) + 0 − 6w = 4w
        pub fn expected_score(&self) -> SignedWork {
            let w = calc_work(0x207fffff);
            SignedWork::from(w * 4u64)
        }
    }
}
