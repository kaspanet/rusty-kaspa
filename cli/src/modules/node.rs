use crate::imports::*;
use kaspa_daemon::KaspadConfig;

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
    async fn defaults() -> Vec<(Self, String)> {
        let mut settings = vec![(Self::Mute, "true".to_string())];

        let root = nw_sys::app::folder();
        if let Ok(binaries) = kaspa_daemon::locate_binaries(&root, "kaspad").await {
            if let Some(path) = binaries.first() {
                settings.push((Self::Location, path.to_string_lossy().to_string()));
            }
        }

        settings
    }
}

// #[derive(Default)]
pub struct Node {
    settings: SettingsStore<KaspadSettings>,
    mute: Arc<AtomicBool>,
    // mute_on_start_triggered: Arc<AtomicBool>,
    // start_flag: Arc<AtomicBool>,
    // lines: Arc<AtomicUsize>,
}

impl Default for Node {
    fn default() -> Self {
        Node {
            settings: SettingsStore::try_new("kaspad.settings").expect("Failed to create miner settings store"),
            mute: Arc::new(AtomicBool::new(true)),
            // mute_on_start_triggered: Arc::new(AtomicBool::new(true)),
            // start_flag: Arc::new(AtomicBool::new(true)),
            // lines: Arc::new(AtomicUsize::new(0)),
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
    async fn create_config(&self, ctx: &Arc<KaspaCli>) -> Result<KaspadConfig> {
        let location: String = self
            .settings
            .get(KaspadSettings::Location)
            .ok_or_else(|| Error::Custom("No miner binary specified, please use `miner select` to select a binary.".into()))?;
        let network_type: NetworkType = ctx.wallet().network()?;
        let mute = self.mute.load(Ordering::SeqCst);
        let config = KaspadConfig::new(location.as_str(), network_type, mute)?;
        Ok(config)
    }

    async fn main(self: Arc<Self>, ctx: Arc<KaspaCli>, mut argv: Vec<String>, _cmd: &str) -> Result<()> {
        if argv.is_empty() {
            return self.display_help(ctx, argv).await;
        }
        let kaspad = ctx.daemons().kaspad();
        match argv.remove(0).as_str() {
            "start" => {
                let mute = self.mute.load(Ordering::SeqCst);
                if mute {
                    tprintln!(ctx, "starting kaspad... {}", style("(logs are muted, use 'node mute' to toggle)").dim());
                } else {
                    tprintln!(ctx, "starting kaspad... {}", style("(use 'node mute' to mute logging)").dim());
                }

                kaspad.configure(self.create_config(&ctx).await?).await?;
                kaspad.start().await?;
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
                kaspad.mute(mute).await?;
                self.settings.set(KaspadSettings::Mute, mute).await?;
            }
            "status" => {
                let status = kaspad.status().await?;
                tprintln!(ctx, "{}", status);
            }
            "select" => {
                self.select(ctx).await?;
            }
            "version" => {
                kaspad.configure(self.create_config(&ctx).await?).await?;
                let version = kaspad.version().await?;
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

    async fn select(self: Arc<Self>, ctx: Arc<KaspaCli>) -> Result<()> {
        let root = nw_sys::app::folder();

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

        Ok(())
    }
}

//     pub async fn handle_stdio(&self, ctx: &Arc<KaspaCli>, stdio: Stdio) -> Result<()> {
//         let text = stdio.trim();

//         if self.mute_on_start.load(Ordering::SeqCst) {
//             if self.mute_on_start_triggered.load(Ordering::SeqCst) {
//                 ctx.term().writeln(trim(text.crlf()));
//             } else {
//                 let lines = text.split("\n").collect::<Vec<_>>();
//                 if self.lines.fetch_add(lines.len(), Ordering::SeqCst) > 10 {
//                     tprintln!(ctx, "... suspending log output (use 'node mute' to resume) ...");
//                     self.mute_on_start_triggered.store(true, Ordering::SeqCst);
//                     ctx.daemons().kaspad().mute(true).await?;
//                 } else {
//                     lines.into_iter().for_each(|line| {
//                         ctx.term().writeln(line);
//                     });
//                 }
//             }
//         } else {
//             ctx.term().writeln(text.crlf());
//         }
//         Ok(())
//     }
// }

// pub fn trim(mut s: String) -> String {
//     // let mut s = String::from(self);
//     if s.ends_with('\n') {
//         s.pop();
//         if s.ends_with('\r') {
//             s.pop();
//         }
//     }
//     s
// }
