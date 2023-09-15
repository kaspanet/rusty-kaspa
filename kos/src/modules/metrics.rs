use crate::imports::*;
use kaspa_cli_lib::metrics::{metrics::MetricsSinkFn, Metrics as Inner};

pub struct Metrics {
    inner: Arc<Inner>,
    core: Arc<CoreIpc>,
}

impl Metrics {
    pub fn new(core: &Arc<CoreIpc>) -> Self {
        Self { core: core.clone(), inner: Arc::new(Inner::default()) }
    }
}

#[async_trait]
impl Handler for Metrics {
    fn verb(&self, ctx: &Arc<dyn Context>) -> Option<&'static str> {
        self.inner.verb(ctx)
    }

    fn help(&self, ctx: &Arc<dyn Context>) -> &'static str {
        self.inner.help(ctx)
    }

    async fn start(self: Arc<Self>, ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.inner.clone().start(ctx).await
    }

    async fn stop(self: Arc<Self>, ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.inner.clone().stop(ctx).await
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.to_string())?;
        Ok(())
    }
}

impl Metrics {
    pub fn register_sink(&self, target: MetricsSinkFn) {
        self.inner.register_sink(target);
    }

    pub fn unregister_sink(&self) {
        self.inner.unregister_sink();
    }

    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            self.inner.display_help(ctx, argv).await?;
            return Ok(());
        }
        match argv.remove(0).as_str() {
            "open" => {
                self.core.metrics_open().await?;
            }
            "close" => {
                self.core.metrics_close().await?;
            }
            "retention" => {
                if argv.is_empty() {
                    tprintln!(ctx, "missing retention value");
                    return Ok(());
                } else {
                    let retention = argv.remove(0).parse::<u64>().map_err(|e| e.to_string())?;
                    if !(1..=168).contains(&retention) {
                        tprintln!(ctx, "retention value must be between 1 and 168 hours");
                        return Ok(());
                    }
                    self.core.metrics_ctl(MetricsCtl::Retention(retention)).await?;
                }
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");

                self.display_help(ctx, argv).await?;
                return Ok(());
            }
        }

        Ok(())
    }

    pub async fn display_help(self: &Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        ctx.term().help(&[("open", "Open metrics window"), ("close", "Close metrics window")], None)?;

        Ok(())
    }
}
