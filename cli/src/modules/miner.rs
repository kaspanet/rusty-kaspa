use kaspa_daemon::KaspadConfig;

use crate::imports::*;

#[derive(Default)]
pub struct Miner;

#[async_trait]
impl Handler for Miner {
    fn verb(&self, ctx: &Arc<dyn Context>) -> Option<&'static str> {
        if let Ok(ctx) = ctx.clone().downcast_arc::<KaspaCli>() {
            ctx.daemons().clone().kaspad.as_ref().map(|_| "node")
        } else {
            None
        }
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Manage the local CPU miner instance"
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Miner {
    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }
        let cpu_miner = ctx.daemons().cpu_miner();
        match argv.remove(0).as_str() {
            "start" => {
                cpu_miner.start().await?;
            }
            "stop" => {
                cpu_miner.stop().await?;
            }
            "restart" => {
                cpu_miner.restart().await?;
            }
            "kill" => {
                cpu_miner.kill().await?;
            }
            "status" => {
                let status = cpu_miner.status().await?;
                tprintln!(ctx, "{}", status);
            }
            "select" => {
                self.select(ctx).await?;
            }
            _ => {
                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        let help = "\n\
            \tstart   - Start the local CPU miner instance\n\
            \tstop    - Stop the local CPU miner instance\n\
            \trestart - Restart the local CPU miner instance\n\
            \tkill    - Kill the local CPU miner instance\n\
            \tstatus  - Get the status of the local CPU miner instance\n\
        \n\
        ";

        tprintln!(ctx, "{}", help.crlf());

        Ok(())
    }

    async fn select(self: Arc<Self>, ctx: Arc<KaspaCli>) -> Result<()> {
        let root = nw_sys::app::folder();

        let binaries = kaspa_daemon::locate_binaries(root.as_str(), "kaspa-cpu-miner").await?;

        if binaries.is_empty() {
            tprintln!(ctx, "No kaspa-cpu-miner binaries found");
        } else {
            let binaries = binaries.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
            if let Some(selection) = ctx.term().select("Please select kaspa-cpu-miner binary", &binaries).await? {
                tprintln!(ctx, "selecting: {}", selection);
                let config = KaspadConfig::new(selection.as_str(), NetworkType::Testnet)?;
                ctx.daemons().kaspad().configure(config).await?;
            } else {
                tprintln!(ctx, "no selection is made");
            }
        }

        Ok(())
    }
}
