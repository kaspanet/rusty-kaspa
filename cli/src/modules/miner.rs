use crate::imports::*;
use kaspa_daemon::{locate_binaries, CpuMinerConfig};
pub use workflow_node::process::Event;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum MinerSettings {
    #[describe("Binary location")]
    Location,
    #[describe("gRPC server (default: 127.0.0.1)")]
    Server,
    #[describe("Mute logs")]
    Mute,
}

#[async_trait]
impl DefaultSettings for MinerSettings {
    async fn defaults() -> Vec<(Self, String)> {
        let mut settings = vec![(Self::Server, "127.0.0.1".to_string()), (Self::Mute, "true".to_string())];

        let root = nw_sys::app::folder();
        if let Ok(binaries) = locate_binaries(&root, "kaspa-cpu-miner").await {
            if let Some(path) = binaries.first() {
                settings.push((Self::Location, path.to_string_lossy().to_string()));
            }
        }

        settings
    }
}

pub struct Miner {
    settings: SettingsStore<MinerSettings>,
    mute: Arc<AtomicBool>,
}

impl Default for Miner {
    fn default() -> Self {
        Miner {
            settings: SettingsStore::try_new("miner.settings").expect("Failed to create miner settings store"),
            mute: Arc::new(AtomicBool::new(true)),
        }
    }
}

#[async_trait]
impl Handler for Miner {
    fn verb(&self, ctx: &Arc<dyn Context>) -> Option<&'static str> {
        if let Ok(ctx) = ctx.clone().downcast_arc::<KaspaCli>() {
            ctx.daemons().clone().cpu_miner.as_ref().map(|_| "miner")
        } else {
            None
        }
    }

    fn help(&self, _ctx: &Arc<dyn Context>) -> &'static str {
        "Manage the local CPU miner instance"
    }

    async fn start(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.settings.try_load().await.ok();
        if let Some(mute) = self.settings.get(MinerSettings::Mute) {
            self.mute.store(mute, Ordering::Relaxed);
        }

        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Miner {
    async fn create_config(&self, ctx: &Arc<KaspaCli>) -> Result<CpuMinerConfig> {
        let location: String = self
            .settings
            .get(MinerSettings::Location)
            .ok_or_else(|| Error::Custom("No miner binary specified, please use `miner select` to select a binary.".into()))?;
        let network_type: NetworkType = ctx.wallet().network()?;
        let address = ctx.account().await?.receive_address().await?;
        let server: String = self.settings.get(MinerSettings::Server).unwrap_or("127.0.0.1".to_string());
        let mute = self.mute.load(Ordering::SeqCst);
        let config = CpuMinerConfig::new(location.as_str(), network_type, address, server, mute)?;
        Ok(config)
    }

    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }
        let cpu_miner = ctx.daemons().cpu_miner();
        match argv.remove(0).as_str() {
            "start" => {
                let mute = self.mute.load(Ordering::SeqCst);
                if mute {
                    tprintln!(ctx, "starting miner... {}", style("(logs are muted, use 'miner mute' to toggle)").dim());
                } else {
                    tprintln!(ctx, "starting miner... {}", style("(use 'miner mute' to mute logging)").dim());
                }

                cpu_miner.configure(self.create_config(&ctx).await?).await?;
                cpu_miner.start().await?;
            }
            "stop" => {
                cpu_miner.stop().await?;
            }
            "restart" => {
                cpu_miner.configure(self.create_config(&ctx).await?).await?;
                cpu_miner.restart().await?;
            }
            "kill" => {
                cpu_miner.kill().await?;
            }
            "mute" => {
                let mute = !self.mute.load(Ordering::SeqCst);
                self.mute.store(mute, Ordering::SeqCst);
                if mute {
                    tprintln!(ctx, "{}", style("node is muted").dim());
                } else {
                    tprintln!(ctx, "{}", style("node is unmuted").dim());
                }
                cpu_miner.mute(mute).await?;
                self.settings.set(MinerSettings::Mute, mute).await?;
            }
            "status" => {
                let status = cpu_miner.status().await?;
                tprintln!(ctx, "{}", status);
            }
            "select" => {
                self.select(ctx).await?;
            }
            "version" => {
                cpu_miner.configure(self.create_config(&ctx).await?).await?;
                let version = cpu_miner.version().await?;
                tprintln!(ctx, "{}", version);
            }
            _ => {
                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        let help = "\n\
            \tselect  - Select CPU miner executable (binary) location\n\
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
                self.settings.set(MinerSettings::Location, selection.as_str()).await?;
            } else {
                tprintln!(ctx, "no selection is made");
            }
        }

        Ok(())
    }

    pub async fn handle_event(&self, ctx: &Arc<KaspaCli>, event: Event) -> Result<()> {
        let term = ctx.term();

        match event {
            Event::Exit(_code) => {
                tprintln!(ctx, "Miner her exited");
            }
            Event::Error(error) => {
                tprintln!(ctx, "Miner error: {error}");
            }
            Event::Stdout(text) | Event::Stderr(text) => {
                let sanitize = true;
                if sanitize {
                    let lines = text.split('\n').collect::<Vec<_>>();
                    lines.into_iter().for_each(|line| {
                        let line = line.trim();
                        if !line.is_empty() {
                            if line.len() < 38 || &line[30..31] != "[" {
                                term.writeln(line);
                            } else {
                                let time = &line[11..23];
                                let kind = &line[31..36];
                                let text = &line[38..];
                                match kind {
                                    "WARN" => {
                                        term.writeln(format!("{time} | {}", style(text).yellow()));
                                    }
                                    "ERROR" => {
                                        term.writeln(format!("{time} | {}", style(text).red()));
                                    }
                                    _ => {
                                        term.writeln(format!("{time} | {text}"));
                                    }
                                }
                            }
                        }
                    });
                } else {
                    term.writeln(format!("Miner: {}", text.trim().crlf()));
                }
            }
        }

        Ok(())
    }
}
