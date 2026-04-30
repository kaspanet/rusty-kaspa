use std::{
    sync::{Arc, Mutex},
    thread::sleep,
    time::{Duration, Instant},
};

use kaspa_consensus::{
    config::ConfigBuilder,
    consensus::Consensus,
    constants::perf::PerfParams,
    model::stores::ghostdag::KType,
    params::{DEVNET_PARAMS, ForkActivation, NETWORK_DELAY_BOUND, Params},
};
use kaspa_consensus_core::{
    BlockLevel,
    api::{ConsensusApi, ImportLane},
    config::bps::calculate_ghostdag_k,
    subnets::SubnetworkId,
    tx::TransactionOutpoint,
};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use kaspa_hashes::{Hash, ZERO_HASH};
use kaspa_smt_store::{processor::SmtStores, streaming_import::streaming_import};
use simpa::simulator::{
    miner::{LaneContext, LaneProducer},
    network::KaspaNetworkSimulator,
};

const BPS: f64 = 1.0;
const DELAY: f64 = 1.0;
const MINERS: u64 = 1;
const TXS_PER_BLOCK: u64 = 1;
const LANE_COUNT: u32 = 2_000;
const LANE_BAND: u32 = 200;
const LANE_EPOCH_BLOCKS: u64 = 55;
const LANE_CARRY_PERCENT: u32 = 10;
const FINALITY_DEPTH: u64 = 100 * 2;
const PRUNING_DEPTH: u64 = 100 * 2 * 2 + 50;
const MIN_TARGET_BLOCKS: u64 = 2_000;
const TARGET_BLOCKS: u64 =
    if MIN_TARGET_BLOCKS > PRUNING_DEPTH + 2 * FINALITY_DEPTH { MIN_TARGET_BLOCKS } else { PRUNING_DEPTH + 2 * FINALITY_DEPTH };
const SMT_CHUNK_SIZE: usize = 4096;
const SMT_PRUNING_SETTLE_TIMEOUT: Duration = Duration::from_secs(30);
const SMT_PRUNING_SETTLE_POLL_INTERVAL: Duration = Duration::from_millis(100);

static REPRO_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn pruning_point_smt_export_import_repro_seed_2() {
    assert_pruning_point_smt_roundtrip(2);
}

#[test]
fn pruning_point_smt_export_import_repro_seed_4() {
    assert_pruning_point_smt_roundtrip(4);
}

#[test]
fn pruning_point_smt_export_import_repro_seed_5() {
    assert_pruning_point_smt_roundtrip(5);
}

fn assert_pruning_point_smt_roundtrip(seed: u64) {
    let _guard = REPRO_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kaspa_core::log::try_init_logger("warn");

    let mut params = DEVNET_PARAMS;
    apply_pruning_repro_params(&mut params);
    let config = Arc::new(
        ConfigBuilder::new(params)
            .apply_args(|config| apply_perf_params(&mut config.perf))
            .adjust_perf_params_to_consensus_params()
            .skip_proof_of_work()
            .enable_sanity_checks()
            .build(),
    );

    let mut sim = KaspaNetworkSimulator::new_with_seed(DELAY, BPS, Some(TARGET_BLOCKS), config, None, Some(seed));
    let (consensus, handles, lifetime) = sim
        .init_with_lane_producer(MINERS, TXS_PER_BLOCK, false, None, None, None, false, |miner_id| {
            Box::new(ChurningLaneProducer::new(seed ^ miner_id, LANE_COUNT, LANE_BAND, LANE_EPOCH_BLOCKS, LANE_CARRY_PERCENT))
        })
        .run(u64::MAX);

    wait_for_smt_pruning_to_settle(&consensus, seed);
    consensus.shutdown(handles);

    let pp = consensus.pruning_point();
    let pp_blue_score = consensus.get_header(pp).unwrap().blue_score;
    let smt_cutoff = pp_blue_score.saturating_sub(FINALITY_DEPTH).saturating_sub(1);
    let metadata = consensus.get_pruning_point_smt_metadata(pp).unwrap();
    let (imported_root, imported_lanes) =
        import_exported_pruning_point_smt(&*consensus, pp, pp_blue_score, metadata.lanes_root, metadata.active_lanes_count);
    let stale = consensus
        .count_stale_smt_entries(smt_cutoff)
        .unwrap_or_else(|e| panic!("seed {seed}: failed to count stale SMT entries at or below cutoff {smt_cutoff}: {e}"));
    if stale.total() > 0 {
        panic!(
            "seed {seed}: stale SMT entries remained at or below cutoff {smt_cutoff}: \
             branch_versions={}, lane_versions={}, score_index={}, total={}",
            stale.branch_versions,
            stale.lane_versions,
            stale.score_index,
            stale.total()
        );
    }
    let metadata_lanes = metadata.active_lanes_count;
    let metadata_root = metadata.lanes_root;

    drop(consensus);
    drop(lifetime);

    assert_eq!(imported_lanes, metadata_lanes, "seed {seed}: imported lane count does not match pruning-point metadata");
    assert_eq!(imported_root, metadata_root, "seed {seed}: imported SMT root does not match pruning-point metadata");
}

fn wait_for_smt_pruning_to_settle(consensus: &Consensus, seed: u64) {
    let start = Instant::now();

    // The simulator stops producing blocks before the pruning processor has
    // necessarily drained its last message. Shutting down here can interrupt a
    // pruning-point move after the pruning point store advances but before SMT
    // pruning completes. The retention checkpoint catches up with the
    // retention-period root only after pruning has completed.
    loop {
        if consensus.is_pruning_stable().unwrap_or_else(|e| panic!("seed {seed}: failed to read pruning stability marker: {e}")) {
            return;
        }

        if start.elapsed() >= SMT_PRUNING_SETTLE_TIMEOUT {
            panic!("seed {seed}: pruning did not settle within {SMT_PRUNING_SETTLE_TIMEOUT:?}");
        }

        sleep(SMT_PRUNING_SETTLE_POLL_INTERVAL);
    }
}

fn apply_pruning_repro_params(params: &mut Params) {
    // Keep this aligned with simpa's `--test-pruning` profile, with covenants
    // enabled so generated non-native lanes update the sequence-commit SMT.
    params.max_block_level = BlockLevel::MAX - 1;
    params.coinbase_maturity = 200;

    let max_delay = DELAY.max(NETWORK_DELAY_BOUND as f64);
    let k = u64::max(calculate_ghostdag_k(2.0 * max_delay * BPS, 0.05), params.ghostdag_k() as u64);
    let k = u64::min(k, KType::MAX as u64) as KType;
    params.ghostdag_k = k;
    params.mergeset_size_limit = k as u64 * 10;
    params.max_block_parents = u8::max((0.66 * k as f64) as u8, 10);
    params.target_time_per_block = (1000.0 / BPS) as u64;
    params.merge_depth = (params.merge_depth as f64 * BPS) as u64;
    params.coinbase_maturity = (params.coinbase_maturity as f64 * f64::max(1.0, BPS * DELAY * 0.25)) as u64;
    params.crescendo_activation = ForkActivation::always();
    params.timestamp_deviation_tolerance = (600.0 * BPS) as u64;
    params.past_median_time_sample_rate = (10.0 * BPS) as u64;
    params.difficulty_sample_rate = (2.0 * BPS) as u64;

    params.pruning_proof_m = 16;
    params.min_difficulty_window_size = 16;
    params.timestamp_deviation_tolerance = 16;
    params.difficulty_window_size = params.difficulty_window_size.min(32);

    params.ghostdag_k = 20;
    params.finality_depth = FINALITY_DEPTH;
    params.merge_depth = 64 * 2;
    params.mergeset_size_limit = 32 * 2;
    params.pruning_depth = PRUNING_DEPTH;

    params.covenants_activation = ForkActivation::always();
    params.storage_mass_parameter = 10_000;
}

fn apply_perf_params(perf: &mut PerfParams) {
    perf.block_processors_num_threads = 2;
    perf.virtual_processor_num_threads = 2;
}

fn import_exported_pruning_point_smt(
    consensus: &dyn ConsensusApi,
    pp: Hash,
    pp_blue_score: u64,
    lanes_root: Hash,
    expected_lane_count: u64,
) -> (Hash, u64) {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = SmtStores::new(db.clone(), 1, 1);
    let mut stream = consensus.open_pruning_point_smt_lane_stream(pp).unwrap();
    let chunks = std::iter::from_fn(move || {
        let mut batch: Vec<ImportLane> = Vec::with_capacity(SMT_CHUNK_SIZE);
        for _ in 0..SMT_CHUNK_SIZE {
            let Some(lane) = stream.next() else { break };
            batch.push(lane.unwrap());
        }
        (!batch.is_empty()).then_some(batch)
    });

    let result = streaming_import(&db, &stores, ZERO_HASH, expected_lane_count, lanes_root, chunks, 4096).unwrap();
    let imported_root = result.root;
    let imported_lanes = result.lanes_imported;

    stores.prune(&db, pp_blue_score);
    let stale = stores.count_entries_at_or_below(pp_blue_score).unwrap();
    assert_eq!(
        stale.total(),
        0,
        "imported SMT stores must be empty after pruning at pruning-point score {pp_blue_score}: \
         branch_versions={}, lane_versions={}, score_index={}, total={}",
        stale.branch_versions,
        stale.lane_versions,
        stale.score_index,
        stale.total()
    );

    (imported_root, imported_lanes)
}

struct ChurningLaneProducer {
    seed: u64,
    lanes: u32,
    lane_band: u32,
    lane_epoch_blocks: u64,
    lane_carry_percent: u32,
}

impl ChurningLaneProducer {
    fn new(seed: u64, lanes: u32, lane_band: u32, lane_epoch_blocks: u64, lane_carry_percent: u32) -> Self {
        let lanes = lanes.max(1);
        Self {
            seed,
            lanes,
            lane_band: lane_band.clamp(1, lanes),
            lane_epoch_blocks: lane_epoch_blocks.max(1),
            lane_carry_percent: lane_carry_percent.min(100),
        }
    }

    fn lane_number(&self, ctx: &LaneContext) -> u32 {
        let x = self.mix(ctx);
        let epoch = ctx.block_index / self.lane_epoch_blocks;
        let mut band_epoch = epoch;

        if epoch > 0 && (x % 100) < self.lane_carry_percent as u64 {
            let carry_back = 1 + ((x >> 8) % epoch.min(3));
            band_epoch = epoch - carry_back;
        }

        let bands = self.lanes.div_ceil(self.lane_band);
        let band = (band_epoch as u32) % bands;
        let start = band * self.lane_band;
        let width = self.lane_band.min(self.lanes - start);
        start + ((x >> 32) as u32 % width) + 1
    }

    fn mix(&self, ctx: &LaneContext) -> u64 {
        let mut x = self.seed
            ^ ctx.sim_time.rotate_left(7)
            ^ ctx.block_index.rotate_left(17)
            ^ ctx.tx_index.rotate_left(29)
            ^ ctx.miner_id.rotate_left(41)
            ^ outpoint_fingerprint(ctx.outpoint);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51afd7ed558ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
        x ^= x >> 33;
        x
    }
}

fn outpoint_fingerprint(outpoint: TransactionOutpoint) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&outpoint.transaction_id.as_bytes()[..8]);
    u64::from_le_bytes(bytes) ^ outpoint.index as u64
}

impl LaneProducer for ChurningLaneProducer {
    fn next_lane(&mut self, ctx: LaneContext) -> SubnetworkId {
        SubnetworkId::from_namespace(self.lane_number(&ctx).to_be_bytes())
    }
}
