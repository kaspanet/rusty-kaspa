use std::pin::Pin;

use workflow_core::task::sleep;

use crate::imports::*;
use kaspa_rpc_core::api::rpc::RpcApi;

// use kaspa_rpc_core::{ConsensusMetrics, ProcessMetrics};
// use workflow_nw::ipc::*;
// use kaspa_metrics::{MetricsCtl, data::MetricsData, result::Result as MetricsResult};
use super::MetricsData;

// pub type MetricsSinkFn = Arc<Box<(dyn Fn(MetricsData))>>;
pub type MetricsSinkFn =
    Arc<Box<dyn Send + Sync + Fn(MetricsData) -> Pin<Box<(dyn Send + 'static + Future<Output = Result<()>>)>> + 'static>>;

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
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            settings: SettingsStore::try_new("metrics.settings").expect("Failed to create miner settings store"),
            mute: Arc::new(AtomicBool::new(true)),
            task_ctl: DuplexChannel::oneshot(),
            rpc: Arc::new(Mutex::new(None)),
            sink: Arc::new(Mutex::new(None)),
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

    async fn start(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.settings.try_load().await.ok();
        if let Some(mute) = self.settings.get(MetricsSettings::Mute) {
            self.mute.store(mute, Ordering::Relaxed);
        }

        self.start_task().await?;
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

    pub async fn start_task(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();

        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();

        spawn(async move {
            loop {
                let poll = sleep(Duration::from_millis(0));

                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },
                    _ = poll.fuse() => {
                        let data = {
                            if let Some(rpc) = this.rpc() {
                                // if let Ok(_metrics) = rpc.get_metrics(true, true).await.map_err(|e| e.to_string()) {
                                if let Ok(_metrics) = rpc.get_metrics(true, true).await {

                                } else {

                                }

                            } else {
                                // - TODO - post zero values...
                            }

                            let data = MetricsData::Noop;
                            Some(data)
                        };

                        if let Some(data) = data {

                            // TODO - output to terminal...

                            if let Some(sink) = this.sink() {
                                sink(data).await.ok();
                            }
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
        let help = "\n\
            \topen  - Open metrics window\n\
            \tclose - Close metrics window\n\
        \n\
        ";

        tprintln!(ctx, "{}", help.crlf());

        Ok(())
    }
}

// #[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
// pub enum MetricsData {
//     Tps(u64),
//     ConsensusMetrics(ConsensusMetrics),
//     ProcessMetrics(ProcessMetrics),
// }

// #[derive(Debug, Clone)]
// pub struct MetricsIpc {
//     target: IpcTarget,
// }

// #[async_trait]
// impl MetricsCtl for MetricsIpc {
//     async fn post_data(&self, data : MetricsData) -> MetricsResult<()> {
//         self.post(data).await
//     }
// }

// impl MetricsIpc {
//     pub fn new(target: IpcTarget) -> MetricsIpc {
//         MetricsIpc { target }
//     }

//     pub async fn post(&self, data: MetricsData) -> IpcResult<()> {
//         self.target.notify(MetricsOps::MetricsData, data).await?;
//         Ok(())
//     }
// }
