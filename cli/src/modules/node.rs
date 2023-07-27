use crate::imports::*;
use kaspa_daemon::KaspadConfig;
use workflow_core::task::sleep;
use workflow_node::process;
pub use workflow_node::process::Event;
use workflow_store::fs;

#[derive(Describe, Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum KaspadSettings {
    #[describe("Binary location")]
    Location,
    #[describe("Mute logs")]
    Mute,
}

#[async_trait]
impl DefaultSettings for KaspadSettings {
    async fn defaults() -> Vec<(Self, Value)> {
        let mut settings = vec![(Self::Mute, to_value(true).unwrap())];

        let root = nw_sys::app::folder();
        if let Ok(binaries) = kaspa_daemon::locate_binaries(&root, "kaspad").await {
            if let Some(path) = binaries.first() {
                settings.push((Self::Location, to_value(path.to_string_lossy().to_string()).unwrap()));
            }
        }

        settings
    }
}

// #[derive(Default)]
pub struct Node {
    settings: SettingsStore<KaspadSettings>,
    mute: Arc<AtomicBool>,
    is_running: Arc<AtomicBool>,
}

impl Default for Node {
    fn default() -> Self {
        Node {
            settings: SettingsStore::try_new("kaspad").expect("Failed to create miner settings store"),
            mute: Arc::new(AtomicBool::new(true)),
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }
}

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
        "Manage the local Kaspa node instance"
    }

    async fn start(self: Arc<Self>, _ctx: &Arc<dyn Context>) -> cli::Result<()> {
        self.settings.try_load().await.ok();
        if let Some(mute) = self.settings.get(KaspadSettings::Mute) {
            self.mute.store(mute, Ordering::Relaxed);
        }
        Ok(())
    }

    async fn handle(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, cmd: &str) -> cli::Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        self.main(ctx, argv, cmd).await.map_err(|e| e.into())
    }
}

impl Node {
    pub fn is_running(&self) -> bool {
        self.is_running.load(Ordering::SeqCst)
    }

    async fn create_config(&self, ctx: &Arc<KaspaCli>) -> Result<KaspadConfig> {
        let location: String = self
            .settings
            .get(KaspadSettings::Location)
            .ok_or_else(|| Error::Custom("No miner binary specified, please use `miner select` to select a binary.".into()))?;
        let network_id = ctx.wallet().network_id()?;
        // disabled for prompt update (until progress events are implemented)
        // let mute = self.mute.load(Ordering::SeqCst);
        let mute = false;
        let config = KaspadConfig::new(location.as_str(), network_id, mute);
        Ok(config)
    }

    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }
        let kaspad = ctx.daemons().kaspad();
        match argv.remove(0).as_str() {
            "start" => {
                let mute = self.mute.load(Ordering::SeqCst);
                if mute {
                    tprintln!(ctx, "starting kaspa node... {}", style("(logs are muted, use 'node mute' to toggle)").dim());
                } else {
                    tprintln!(ctx, "starting kaspa node... {}", style("(use 'node mute' to mute logging)").dim());
                }

                kaspad.configure(self.create_config(&ctx).await?).await?;
                kaspad.start().await?;

                spawn(async move {
                    sleep(Duration::from_millis(1000)).await;
                    ctx.term().exec("connect".to_string()).await.ok();
                });
            }
            "stop" => {
                kaspad.stop().await?;
            }
            "restart" => {
                kaspad.configure(self.create_config(&ctx).await?).await?;
                kaspad.restart().await?;
            }
            "kill" => {
                kaspad.kill().await?;
            }
            "mute" | "logs" => {
                let mute = !self.mute.load(Ordering::SeqCst);
                self.mute.store(mute, Ordering::SeqCst);
                if mute {
                    tprintln!(ctx, "{}", style("node is muted").dim());
                } else {
                    tprintln!(ctx, "{}", style("node is unmuted").dim());
                }
                // kaspad.mute(mute).await?;
                self.settings.set(KaspadSettings::Mute, mute).await?;
            }
            "status" => {
                let status = kaspad.status().await?;
                tprintln!(ctx, "{}", status);
            }
            "select" => {
                let regex = Regex::new(r"(?i)^\s*node\s*select\s*").unwrap();
                let path = regex.replace(cmd, "").trim().to_string();
                self.select(ctx, path.is_empty().then_some(path)).await?;
            }
            "version" => {
                kaspad.configure(self.create_config(&ctx).await?).await?;
                let version = kaspad.version().await?;
                tprintln!(ctx, "{}", version);
            }
            v => {
                tprintln!(ctx, "unknown command: '{v}'\r\n");

                return self.display_help(ctx, argv).await;
            }
        }

        Ok(())
    }

    async fn display_help(self: Arc<Self>, ctx: Arc<KaspaCli>, _argv: Vec<String>) -> Result<()> {
        let help = "\n\
            \tselect  - Select Kaspad executable (binary) location\n\
            \tversion - Display Kaspad executable version\n\
            \tstart   - Start the local Kaspa node instance\n\
            \tstop    - Stop the local Kaspa node instance\n\
            \trestart - Restart the local Kaspa node instance\n\
            \tkill    - Kill the local Kaspa node instance\n\
            \tstatus  - Get the status of the local Kaspa node instance\n\
            \tmute    - Toggle log output\n\
        \n\
        ";

        tprintln!(ctx, "{}", help.crlf());

        Ok(())
    }

    async fn select(self: Arc<Self>, ctx: Arc<KaspaCli>, path: Option<String>) -> Result<()> {
        let root = nw_sys::app::folder();

        match path {
            None => {
                let binaries = kaspa_daemon::locate_binaries(root.as_str(), "kaspad").await?;

                if binaries.is_empty() {
                    tprintln!(ctx, "No kaspad binaries found");
                } else {
                    let binaries = binaries.iter().map(|p| p.display().to_string()).collect::<Vec<_>>();
                    if let Some(selection) = ctx.term().select("Please select a kaspad binary", &binaries).await? {
                        tprintln!(ctx, "selecting: {}", selection);
                        self.settings.set(KaspadSettings::Location, selection.as_str()).await?;
                        // let config = KaspadConfig::new(selection.as_str(), NetworkType::Testnet)?;
                        // ctx.daemons().kaspad().configure(config).await?;
                    } else {
                        tprintln!(ctx, "no selection is made");
                    }
                }
            }
            Some(path) => {
                if fs::exists(&path).await? {
                    let version = process::version(&path).await?;
                    tprintln!(ctx, "detected binary version: {}", version);
                    tprintln!(ctx, "selecting: {path}");
                    self.settings.set(KaspadSettings::Location, path.as_str()).await?;
                } else {
                    twarnln!(ctx, "destination binary not found, please specify full path including the binary name");
                    twarnln!(ctx, "example: 'node select /home/user/testnet/kaspad'");
                    tprintln!(ctx, "no selection is made");
                }
            }
        }

        Ok(())
    }

    pub async fn handle_event(&self, ctx: &Arc<KaspaCli>, event: Event) -> Result<()> {
        let term = ctx.term();

        match event {
            Event::Start => {
                self.is_running.store(true, Ordering::SeqCst);
                term.refresh_prompt();
            }
            Event::Exit(_code) => {
                tprintln!(ctx, "Kaspad has exited");
                self.is_running.store(false, Ordering::SeqCst);
                term.refresh_prompt();
            }
            Event::Error(error) => {
                tprintln!(ctx, "{}", style(format!("Kaspad error: {error}")).red());
                self.is_running.store(false, Ordering::SeqCst);
                term.refresh_prompt();
            }
            Event::Stdout(text) | Event::Stderr(text) => {
                if !ctx.wallet().utxo_processor().is_synced() {
                    ctx.wallet().utxo_processor().sync_proc().handle_stdout(&text).await?;
                }

                if !self.mute.load(Ordering::SeqCst) {
                    let sanitize = true;
                    if sanitize {
                        // let text: String = stdio.into();
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
                                    // 𐤊
                                    match kind {
                                        "WARN " => {
                                            term.writeln(format!("{time} {}", style(text).yellow()));
                                        }
                                        "ERROR" => {
                                            term.writeln(format!("{time} {}", style(text).red()));
                                        }
                                        _ => {
                                            term.writeln(format!("{time} {text}"));
                                        }
                                    }
                                }
                            }
                        });
                    } else {
                        term.writeln(text.trim().crlf());
                    }
                }
            }
        }
        Ok(())
    }
}
