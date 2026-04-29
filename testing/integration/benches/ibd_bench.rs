use clap::{ArgGroup, Parser};
use criterion::Criterion;
use kaspa_consensus::consensus::factory::{ConsensusEntryType, MultiConsensusManagementStore};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_core::info;
use kaspa_database::prelude::ConnBuilder;
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_testing_integration::common::daemon::Daemon;
use kaspa_utils::fd_budget;
use kaspad_lib::args::Args;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tempfile::TempDir;

const DATA_DIR_NAME: &str = "datadir";
const CONSENSUS_DIR_NAME: &str = "consensus";
const META_DIR_NAME: &str = "meta";
const SEEDED_CONSENSUS_DIR_NAME: &str = "consensus-001";

#[derive(Parser, Debug)]
#[command(group(ArgGroup::new("network").args(["mainnet", "devnet", "simnet", "testnet"]).multiple(false)))]
struct IbdBenchArgs {
    /// Optional path to a JSON file with override consensus params
    #[arg(long, value_name = "FILE")]
    override_params_file: Option<PathBuf>,

    /// Path to a consensus RocksDB directory
    #[arg(
        long,
        value_name = "DIR",
        long_help = r#"Path to a consensus RocksDB directory.

For a regular kaspad network DB, pass the active consensus directory:
<DATADIR>/consensus/consensus-00X

By default, <DATADIR> is:
Linux: ~/.rusty-kaspa/kaspa-<NET>/datadir
Windows: %LOCALAPPDATA%\rusty-kaspa\kaspa-<NET>\datadir

For example:
--db ~/.rusty-kaspa/kaspa-testnet-10/datadir/consensus/consensus-001

For a simpa-generated DB, pass the exact simpa output directory. For example, if simpa writes to /tmp/simpa, pass:
--db /tmp/simpa"#
    )]
    db: PathBuf,

    /// Use mainnet consensus params
    #[arg(long)]
    mainnet: bool,

    /// Use devnet consensus params
    #[arg(long)]
    devnet: bool,

    /// Use simnet consensus params
    #[arg(long)]
    simnet: bool,

    /// Use testnet consensus params
    #[arg(long)]
    testnet: bool,

    /// Network suffix for networks that support one
    #[arg(long, default_value_t = 10)]
    netsuffix: u32,

    /// Maximum benchmark runtime before failing
    #[arg(long, default_value_t = 1800)]
    timeout_secs: u64,

    /// Poll interval for sync progress
    #[arg(long, default_value_t = 250)]
    poll_interval_millis: u64,

    // Cargo/libtest appends --bench to the benchmark executable invocation. Accept it explicitly so clap stays strict
    // for benchmark-specific arguments while tolerating the harness flag.
    #[arg(long, hide = true)]
    bench: bool,
}

impl IbdBenchArgs {
    fn from_env_args() -> Self {
        let args: Vec<OsString> = std::env::args_os().collect();
        let args_start = args.iter().position(|x| x == "--").map_or(1, |x| x + 1);
        Self::parse_from(std::iter::once(OsString::from("ibd-bench")).chain(args.into_iter().skip(args_start)))
    }

    fn base_args(&self) -> Args {
        let network = self.network_type();
        Args {
            devnet: network == NetworkType::Devnet,
            simnet: network == NetworkType::Simnet,
            testnet: network == NetworkType::Testnet,
            testnet_suffix: self.netsuffix,
            disable_upnp: true,
            disable_dns_seeding: true,
            outbound_target: 0,
            unsafe_rpc: true,
            yes: true,
            override_params_file: self.override_params_file.as_ref().map(|path| path.display().to_string()),
            ..Default::default()
        }
    }

    fn network_type(&self) -> NetworkType {
        if self.mainnet {
            NetworkType::Mainnet
        } else if self.simnet {
            NetworkType::Simnet
        } else if self.testnet {
            NetworkType::Testnet
        } else {
            NetworkType::Devnet
        }
    }
}

struct IbdBenchFixture {
    syncer: Daemon,
    syncer_client: GrpcClient,
    base_args: Args,
    fd_total_budget: i32,
    timeout: Duration,
    poll_interval: Duration,

    target_sink: String,
    target_block_count: u64,
    _syncer_appdir: TempDir,
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn initialize_active_consensus_in_meta_db(meta_db_dir: &Path) {
    let builder = ConnBuilder::default().with_db_path(meta_db_dir.to_path_buf()).with_files_limit(32_i32);
    let management_db = builder.build().unwrap();
    let mut management_store = MultiConsensusManagementStore::new(management_db);

    if management_store.active_consensus_dir_name().unwrap().is_none() {
        let entry = match management_store.active_consensus_entry().unwrap() {
            ConsensusEntryType::New(entry) => entry,
            ConsensusEntryType::Existing(_) => unreachable!("active consensus unexpectedly exists"),
        };
        management_store.save_new_active_consensus(entry).unwrap();
    }
}

fn seed_syncer_database(appdir: &Path, network: NetworkId, db: &Path) {
    assert!(db.exists(), "db path does not exist: {}", db.display());
    assert!(db.is_dir(), "db path is not a directory: {}", db.display());

    let network_data_dir = appdir.join(network.to_prefixed()).join(DATA_DIR_NAME);

    let consensus_root = network_data_dir.join(CONSENSUS_DIR_NAME);
    let target_consensus_dir = consensus_root.join(SEEDED_CONSENSUS_DIR_NAME);
    let meta_db_dir = network_data_dir.join(META_DIR_NAME);

    if target_consensus_dir.exists() {
        fs::remove_dir_all(&target_consensus_dir).unwrap();
    }
    fs::create_dir_all(&consensus_root).unwrap();
    copy_dir_all(db, &target_consensus_dir).unwrap();

    if meta_db_dir.exists() {
        fs::remove_dir_all(&meta_db_dir).unwrap();
    }
    fs::create_dir_all(&meta_db_dir).unwrap();
    initialize_active_consensus_in_meta_db(&meta_db_dir);
}

impl IbdBenchFixture {
    async fn setup(bench_args: &IbdBenchArgs) -> Self {
        if let Some(override_params_file) = bench_args.override_params_file.as_ref() {
            assert!(override_params_file.is_file(), "override params file does not exist: {}", override_params_file.display());
        }
        assert!(bench_args.db.is_dir(), "db directory does not exist: {}", bench_args.db.display());

        let fd_total_budget = (fd_budget::limit() / 2).max(64);
        let base_args = bench_args.base_args();

        let mut syncer_args = base_args.clone();
        Daemon::fill_args_with_random_ports(&mut syncer_args);
        let syncer_appdir = TempDir::new().unwrap();
        syncer_args.appdir = Some(syncer_appdir.path().to_str().unwrap().to_owned());

        info!("Setting up syncer with appdir: {}", syncer_args.appdir.as_ref().unwrap());
        seed_syncer_database(syncer_appdir.path(), syncer_args.network(), &bench_args.db);

        let mut syncer = Daemon::new_random_with_args(syncer_args, fd_total_budget);
        let syncer_client = syncer.start().await;
        let syncer_tip = syncer_client.get_block_dag_info().await.unwrap();
        assert!(syncer_tip.block_count > 0, "syncer loaded a zero-height DB; regenerate the DB with non-trivial block count");

        info!("Syncer started. target sink: {}, target block_count: {}", syncer_tip.sink, syncer_tip.block_count);

        Self {
            syncer,
            syncer_client,
            base_args,
            fd_total_budget,
            timeout: Duration::from_secs(bench_args.timeout_secs),
            poll_interval: Duration::from_millis(bench_args.poll_interval_millis),
            target_sink: syncer_tip.sink.to_string(),
            target_block_count: syncer_tip.block_count,
            _syncer_appdir: syncer_appdir,
        }
    }

    async fn measure_syncee_sync_time(&self) -> Duration {
        let start = Instant::now();

        let mut syncee = Daemon::new_random_with_args(self.base_args.clone(), self.fd_total_budget);
        let syncee_client = syncee.start().await;
        syncee_client.add_peer(format!("127.0.0.1:{}", self.syncer.p2p_port).try_into().unwrap(), true).await.unwrap();

        info!("Syncee connected to syncer. measuring sync time...");

        loop {
            let syncee_tip = syncee_client.get_block_dag_info().await.unwrap();
            if syncee_tip.sink.to_string() == self.target_sink {
                let elapsed = start.elapsed();
                let syncee_info = syncee_client.get_server_info().await.unwrap();

                info!(
                    "Syncee fully synced in {:?}. final block_count: {}, is_synced flag: {}",
                    elapsed, syncee_tip.block_count, syncee_info.is_synced
                );
                println!(
                    "ibd_benchmark_result elapsed_ms={} block_count={} sink={} is_synced={}",
                    elapsed.as_millis(),
                    syncee_tip.block_count,
                    syncee_tip.sink,
                    syncee_info.is_synced
                );

                syncee_client.disconnect().await.unwrap();
                syncee.shutdown();
                return elapsed;
            }

            if start.elapsed() > self.timeout {
                panic!(
                    "timed out after {:?}. syncee block_count={}, sink={}, expected block_count={}, expected sink={}",
                    self.timeout, syncee_tip.block_count, syncee_tip.sink, self.target_block_count, self.target_sink
                );
            }

            tokio::time::sleep(self.poll_interval).await;
        }
    }

    async fn shutdown(mut self) {
        self.syncer_client.disconnect().await.unwrap();
        self.syncer.shutdown();
    }
}

fn bench_syncee_ibd_time_from_db(c: &mut Criterion) {
    kaspa_core::log::try_init_logger("info,kaspa_core::time=debug");
    kaspa_core::panic::configure_panic();

    let bench_args = IbdBenchArgs::from_env_args();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();

    // Syncer setup runs exactly once and is excluded from Criterion's measured samples.
    let fixture = runtime.block_on(IbdBenchFixture::setup(&bench_args));

    let mut group = c.benchmark_group("syncee_ibd_time");
    group.sample_size(10);
    group.bench_function("bench_syncee_ibd_time_from_db", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                total += runtime.block_on(fixture.measure_syncee_sync_time());
            }
            total
        });
    });
    group.finish();

    runtime.block_on(fixture.shutdown());
}

// Run with `cargo bench --package kaspa-testing-integration --bench ibd_bench -- --override-params-file /path/to/override_params.json --db /path/to/db`
fn main() {
    let mut criterion = Criterion::default();
    bench_syncee_ibd_time_from_db(&mut criterion);
    criterion.final_summary();
}
