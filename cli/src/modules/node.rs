use kaspa_daemon::KaspadConfig;

use crate::imports::*;

#[derive(Default)]
pub struct Node;

#[async_trait]
impl Handler for Node {
    fn verb(&self, ctx: &Arc<dyn Context>) -> Option<&'static str> {
        if let Ok(ctx) = ctx.clone().downcast_arc::<KaspaCli>() {
            ctx.daemons().clone().kaspad.as_ref().map(|_| "node")
        } else {
            None
        }
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Manage local Kaspa node instance"
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Node {
    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }
        let kaspad = ctx.daemons().kaspad();
        match argv.remove(0).as_str() {
            "start" => {
                kaspad.start().await?;
            }
            "stop" => {
                kaspad.stop().await?;
            }
            "restart" => {
                kaspad.restart().await?;
            }
            "kill" => {
                kaspad.kill().await?;
            }
            "status" => {
                let status = kaspad.status().await?;
                tprintln!(ctx, "{}", status);
            }
            "select" => {
                // let status = kaspad.status().await?;
                // tprintln!(ctx, "{}", status);
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
            \tstart   - Start local Kaspa node instance\n\
            \tstop    - Stop local Kaspa node instance\n\
            \trestart - Restart local Kaspa node instance\n\
            \tkill    - Kill local Kaspa node instance\n\
            \tstatus  - Get the status of local Kaspa node instance\n\
        \n\
        ";

        tprintln!(ctx, "{}", help.crlf());

        Ok(())
    }

    async fn select(self: Arc<Self>, ctx: Arc<KaspaCli>) -> Result<()> {
        let root = nw_sys::app::folder();

        log_info!("root: `{root}`");

        let binaries = kaspa_daemon::locate_binaries(root.as_str(), "kaspad").await?;

        if binaries.is_empty() {
            tprintln!(ctx, "No kaspad binaries found");
        } else {
            let binaries = binaries.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
            if let Some(selection) = ctx.term().select("Please select a kaspad binary", &binaries).await? {
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
