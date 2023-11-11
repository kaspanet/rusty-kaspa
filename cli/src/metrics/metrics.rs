use super::{MetricsData, MetricsSnapshot};
use crate::imports::*;
use futures::{future::join_all, pin_mut};
use kaspa_rpc_core::{api::rpc::RpcApi, GetMetricsResponse};
use std::pin::Pin;
use workflow_core::{runtime::is_nw, task::interval};
pub type MetricsSinkFn =
    Arc<Box<dyn Send + Sync + Fn(MetricsSnapshot) -> Pin<Box<(dyn Send + 'static + Future<Output = Result<()>>)>> + 'static>>;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum MetricsSettings {
    #[describe("Mute logs")]
    Mute,
}

#[async_trait]
impl DefaultSettings for MetricsSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        // let mut settings = vec![(Self::Mute, "false".to_string())];
        // settings
        vec![]
    }
}

pub struct Metrics {
    settings: SettingsStore<MetricsSettings>,
    mute: Arc<AtomicBool>,
    task_ctl: DuplexChannel,
    rpc: Arc<Mutex<Option<Arc<dyn RpcApi>>>>,
    // target : Arc<Mutex<Option<Arc<dyn MetricsCtl>>>>,
    sink: Arc<Mutex<Option<MetricsSinkFn>>>,
    data: Arc<Mutex<Option<MetricsData>>>,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            settings: SettingsStore::try_new("metrics").expect("Failed to create miner settings store"),
            mute: Arc::new(AtomicBool::new(true)),
            task_ctl: DuplexChannel::oneshot(),
            rpc: Arc::new(Mutex::new(None)),
            sink: Arc::new(Mutex::new(None)),
            data: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Handler for Metrics {
    fn verb(&self, _ctx: &Arc<dyn Context>) -> Option<&'static str> {
        Some("metrics")
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Manage metrics monitoring"
    }

    async fn start(self: Arc<Self>, ctx: &Arc<dyn Context>) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;

        self.settings.try_load().await.ok();
        if let Some(mute) = self.settings.get(MetricsSettings::Mute) {
            self.mute.store(mute, Ordering::Relaxed);
        }

        self.rpc.lock().unwrap().replace(ctx.wallet().rpc_api().clone());

        self.start_task(&ctx).await?;
        Ok(())
    }

    async fn stop(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.stop_task().await?;
        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Metrics {
    fn rpc(&self) -> Option<Arc<dyn RpcApi>> {
        self.rpc.lock().unwrap().clone()
    }

    pub fn register_sink(&self, target: MetricsSinkFn) {
        self.sink.lock().unwrap().replace(target);
    }

    pub fn unregister_sink(&self) {
        self.sink.lock().unwrap().take();
    }

    pub fn sink(&self) -> Option<MetricsSinkFn> {
        self.sink.lock().unwrap().clone()
    }

    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }
        match argv.remove(0).as_str() {
            "open" => {}
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");

                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    pub async fn start_task(self: &Arc<Self>, ctx: &Arc<KaspaCli>) -> Result<()> {
        let this = self.clone();
        let ctx = ctx.clone();

        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();

        *this.data.lock().unwrap() = Some(MetricsData::new(unixtime_as_millis_f64()));

        spawn(async move {
            let interval = interval(Duration::from_secs(1));
            pin_mut!(interval);

            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    _ = interval.next().fuse() => {

                        if !ctx.is_connected() {
                            continue;
                        }

                        let last_data = this.data.lock().unwrap().take().unwrap();
                        this.data.lock().unwrap().replace(MetricsData::new(unixtime_as_millis_f64()));
                        if let Some(rpc) = this.rpc() {
                            let samples = vec![
                                this.sample_metrics(rpc.clone()).boxed(),
                                this.sample_gbdi(rpc.clone()).boxed(),
                                this.sample_cpi(rpc.clone()).boxed(),
                            ];

                            join_all(samples).await;
                        }

                        if let Some(sink) = this.sink() {
                            let snapshot = MetricsSnapshot::from((&last_data, this.data.lock().unwrap().as_ref().unwrap()));
                            sink(snapshot).await.ok();
                        }
                    }
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });
        Ok(())
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.task_ctl.signal(()).await.expect("Metrics::stop_task() signal error");
        Ok(())
    }

    pub async fn display_help(self: &Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        // disable help in non-nw environments
        if !is_nw() {
            return Ok(());
        }

        ctx.term().help(&[("open", "Open metrics window"), ("close", "Close metrics window")], None)?;

        Ok(())
    }

    // --- samplers

    async fn sample_metrics(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<()> {
        if let Ok(metrics) = rpc.get_metrics(true, true).await {
            let GetMetricsResponse { server_time: _, consensus_metrics, process_metrics } = metrics;

            let mut data = self.data.lock().unwrap();
            let data = data.as_mut().unwrap();
            if let Some(consensus_metrics) = consensus_metrics {
                data.blocks_submitted = consensus_metrics.blocks_submitted;
                data.header_counts = consensus_metrics.header_counts;
                data.dep_counts = consensus_metrics.dep_counts;
                data.body_counts = consensus_metrics.body_counts;
                data.txs_counts = consensus_metrics.txs_counts;
                data.chain_block_counts = consensus_metrics.chain_block_counts;
                data.mass_counts = consensus_metrics.mass_counts;
            }

            if let Some(process_metrics) = process_metrics {
                data.resident_set_size_bytes = process_metrics.resident_set_size;
                data.virtual_memory_size_bytes = process_metrics.virtual_memory_size;
                data.cpu_cores = process_metrics.core_num;
                data.cpu_usage = process_metrics.cpu_usage;
                data.fd_num = process_metrics.fd_num;
                data.disk_io_read_bytes = process_metrics.disk_io_read_bytes;
                data.disk_io_write_bytes = process_metrics.disk_io_write_bytes;
                data.disk_io_read_per_sec = process_metrics.disk_io_read_per_sec;
                data.disk_io_write_per_sec = process_metrics.disk_io_write_per_sec;
            }
        }

        Ok(())
    }

    async fn sample_gbdi(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<()> {
        if let Ok(gdbi) = rpc.get_block_dag_info().await {
            let mut data = self.data.lock().unwrap();
            let data = data.as_mut().unwrap();
            data.block_count = gdbi.block_count;
            // data.header_count = gdbi.header_count;
            data.tip_hashes = gdbi.tip_hashes.len();
            data.difficulty = gdbi.difficulty;
            data.past_median_time = gdbi.past_median_time;
            data.virtual_parent_hashes = gdbi.virtual_parent_hashes.len();
            data.virtual_daa_score = gdbi.virtual_daa_score;
        }

        Ok(())
    }

    async fn sample_cpi(self: &Arc<Self>, rpc: Arc<dyn RpcApi>) -> Result<()> {
        if let Ok(_cpi) = rpc.get_connected_peer_info().await {
            // let mut data = self.data.lock().unwrap();
            // - TODO - fold peers into inbound / outbound...
        }

        Ok(())
    }
}
