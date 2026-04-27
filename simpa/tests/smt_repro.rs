use std::sync::{Arc, Mutex};

use kaspa_consensus::{
    config::ConfigBuilder,
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
const TARGET_BLOCKS: u64 = 5_000;
const LANE_COUNT: u32 = 2_000;
const LANE_BAND: u32 = 200;
const LANE_EPOCH_BLOCKS: u64 = 55;
const LANE_CARRY_PERCENT: u32 = 10;
const FINALITY_DEPTH: u64 = 100 * 2;
const PRUNING_DEPTH: u64 = 100 * 2 * 2 + 50;

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
    consensus.shutdown(handles);

    let pp = consensus.pruning_point();
    let pp_header = consensus.get_header(pp).unwrap();
    let metadata = consensus.get_pruning_point_smt_metadata(pp).unwrap();
    let (chunks, exported_lanes) = export_smt_lanes(&*consensus, pp);

    let (import_lifetime, import_db) = create_temp_db!(ConnBuilder::default().with_files_limit(32));
    let import_stores = SmtStores::new(import_db.clone(), 1, 1);
    let result = streaming_import(
        &import_db,
        &import_stores,
        pp_header.blue_score,
        ZERO_HASH,
        metadata.active_lanes_count,
        metadata.lanes_root,
        chunks.into_iter(),
        4096,
    )
    .expect("streaming_import failed");

    let imported_lanes = result.lanes_imported;
    let imported_root = result.root;
    let metadata_lanes = metadata.active_lanes_count;
    let metadata_root = metadata.lanes_root;

    drop(import_stores);
    drop(import_db);
    drop(import_lifetime);
    drop(consensus);
    drop(lifetime);

    assert_eq!(exported_lanes, metadata_lanes, "seed {seed}: exported lane count does not match pruning-point metadata");
    assert_eq!(imported_lanes, metadata_lanes, "seed {seed}: imported lane count does not match pruning-point metadata");
    assert_eq!(imported_root, metadata_root, "seed {seed}: imported SMT root does not match pruning-point metadata");
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

fn export_smt_lanes(consensus: &dyn ConsensusApi, pp: Hash) -> (Vec<Vec<ImportLane>>, u64) {
    const CHUNK_LEN: usize = 4096;

    let stream = consensus.open_pruning_point_smt_lane_stream(pp).unwrap();
    let mut chunks = Vec::new();
    let mut chunk = Vec::with_capacity(CHUNK_LEN);
    let mut count = 0u64;

    for lane in stream {
        chunk.push(lane.unwrap());
        count += 1;
        if chunk.len() == CHUNK_LEN {
            chunks.push(std::mem::replace(&mut chunk, Vec::with_capacity(CHUNK_LEN)));
        }
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }

    (chunks, count)
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

impl LaneProducer for ChurningLaneProducer {
    fn next_lane(&mut self, ctx: LaneContext) -> SubnetworkId {
        SubnetworkId::from_namespace(self.lane_number(&ctx).to_be_bytes())
    }
}

fn outpoint_fingerprint(outpoint: TransactionOutpoint) -> u64 {
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&outpoint.transaction_id.as_bytes()[..8]);
    u64::from_le_bytes(bytes) ^ outpoint.index as u64
}
