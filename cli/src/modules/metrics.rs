use crate::imports::*;
use kaspa_metrics_core::{Metrics as MetricsProcessor, MetricsSinkFn};
use workflow_core::runtime::is_nw;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum MetricsSettings {
    #[describe("Mute logs")]
    Mute,
}

#[async_trait]
impl DefaultSettings for MetricsSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        vec![]
    }
}

pub struct Metrics {
    settings: SettingsStore<MetricsSettings>,
    mute: Arc<AtomicBool>,
    metrics: Arc<MetricsProcessor>,
}

impl Default for Metrics {
    fn default() -> Self {
        Metrics {
            settings: SettingsStore::try_new("metrics").expect("Failed to create miner settings store"),
            mute: Arc::new(AtomicBool::new(true)),
            metrics: Arc::new(MetricsProcessor::default()),
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

        self.metrics.bind_rpc(Some(ctx.wallet().rpc_api().clone()));

        Ok(())
    }

    async fn stop(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.metrics.stop_task().await.map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Metrics {
    pub fn register_sink(&self, target: MetricsSinkFn) {
        self.metrics.register_sink(target);
    }

    pub fn unregister_sink(&self) {
        self.metrics.unregister_sink();
    }

    pub fn sink(&self) -> Option<MetricsSinkFn> {
        self.metrics.sink()
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

    pub async fn display_help(self: &Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        // disable help in non-nw environments
        if !is_nw() {
            return Ok(());
        }

        ctx.term().help(&[("open", "Open metrics window"), ("close", "Close metrics window")], None)?;

        Ok(())
    }

    // --- samplers
}
