use crate::imports::*;
use kaspa_cli::metrics::MetricsData;

static mut TERMINAL: Option<Arc<Terminal>> = None;

#[derive(Clone)]
pub struct Terminal {
    pub inner: Arc<Application>,
    pub ipc: Arc<Ipc<TermOps>>,
    pub core: Arc<CoreIpc>,
    pub cli: Arc<KaspaCli>,
    pub window: Arc<Window>,
    pub callbacks: CallbackMap,
    pub settings: Arc<SettingsStore<TerminalSettings>>,
    pub layout: Arc<Layout<SettingsStore<TerminalSettings>>>,
    pub metrics: Arc<Mutex<Option<Arc<IpcTarget>>>>,
}

impl Terminal {
    pub async fn try_new() -> Result<Arc<Self>> {
        let core_ipc_target = get_ipc_target(Modules::Core).await?.expect("Unable to aquire background window");
        let core = Arc::new(CoreIpc::new(core_ipc_target));
        let daemons = Arc::new(Daemons::new().with_kaspad(core.clone()).with_cpu_miner(core.clone()));

        let settings = Arc::new(SettingsStore::<TerminalSettings>::try_new("terminal.settings")?);
        settings.try_load().await?;
        let font_size = settings.get::<f64>(TerminalSettings::FontSize);

        let terminal_options = TerminalOptions { font_size, ..TerminalOptions::default() };
        let options = KaspaCliOptions::new(terminal_options, Some(daemons));
        let cli = KaspaCli::try_new_arc(options).await?;

        let window = Arc::new(nw_sys::window::get());
        let layout = Arc::new(Layout::try_new(&window, &settings).await?);

        let app = Arc::new(Self {
            inner: Application::new()?,
            ipc: Ipc::try_new_window_binding(&window, Modules::Terminal)?,
            core,
            cli,
            window,
            callbacks: CallbackMap::default(),
            settings,
            layout,
            metrics: Arc::new(Mutex::new(None)),
        });

        unsafe {
            TERMINAL = Some(app.clone());
        };

        Ok(app)
    }

    fn register_ipc_handlers(self: &Arc<Self>) -> Result<()> {
        self.ipc.method(
            TermOps::TestTerminal,
            Method::new(move |args: TestReq| {
                Box::pin(async move {
                    let resp: TestResp = TestResp { resp: args.req + " - response from terminal!" };
                    Ok(resp)
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            TermOps::FontCtl,
            Method::new(move |args: FontCtl| {
                let this = this.clone();
                Box::pin(async move {
                    match args {
                        FontCtl::IncreaseSize => {
                            this.cli.term().increase_font_size().map_err(|e| e.to_string())?;
                            if let Some(font_size) = this.cli.term().get_font_size().unwrap() {
                                this.settings
                                    .set(TerminalSettings::FontSize, font_size)
                                    .await
                                    .expect("Unable to store application settings");
                            }
                        }
                        FontCtl::DecreaseSize => {
                            this.cli.term().decrease_font_size().map_err(|e| e.to_string())?;
                            if let Some(font_size) = this.cli.term().get_font_size().unwrap() {
                                this.settings
                                    .set(TerminalSettings::FontSize, font_size)
                                    .await
                                    .expect("Unable to store application settings");
                            }
                        }
                    }
                    Ok(())
                })
            }),
        );

        let this = self.clone();
        self.ipc.method(
            TermOps::EditCtl,
            Method::new(move |args: EditCtl| {
                let this = this.clone();
                Box::pin(async move {
                    match args {
                        EditCtl::Copy => {
                            this.cli.term().clipboard_copy().map_err(|e| e.to_string())?;
                        }
                        EditCtl::Paste => {
                            this.cli.term().clipboard_paste().map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(())
                })
            }),
        );

        let this = self.clone();
        self.ipc.notification(
            TermOps::MetricsCtl,
            Notification::new(move |args: MetricsSinkCtl| {
                let this = this.clone();
                Box::pin(async move {
                    let metrics = this.cli.handlers().get("metrics").expect("MetricsCtlSink: missing metrics module");
                    let metrics =
                        metrics.downcast_arc::<crate::modules::metrics::Metrics>().expect("MetricsCtlSink: invalid metrics module");
                    match args {
                        MetricsSinkCtl::Activate => {
                            let ipc = get_ipc_target(Modules::Metrics)
                                .await
                                .expect("Error actuiring ipc for the metrics window")
                                .expect("Unable to locate ipc for the metrics window");

                            this.metrics.lock().unwrap().replace(Arc::new(ipc));
                            metrics.register_sink(Arc::new(Box::new(move |data: MetricsData| {
                                let this = this.clone();

                                Box::pin(async move {
                                    let ipc = this.metrics.lock().unwrap().as_ref().unwrap().clone();
                                    ipc.notify(MetricsOps::MetricsData, data).await.unwrap_or_else(|err| {
                                        log_error!("error posting metrics data to metrics window: {:?}", err);
                                    });

                                    Ok(())
                                })
                            })))
                        }
                        MetricsSinkCtl::Deactivate => {
                            this.metrics.lock().unwrap().take();
                            metrics.unregister_sink();
                        }
                    }
                    Ok(())
                })
            }),
        );

        let this = self.clone();
        self.ipc.notification(
            TermOps::DaemonEvent,
            Notification::new(move |event: DaemonEvent| {
                let this = this.clone();
                Box::pin(async move {
                    this.cli
                        .handle_daemon_event(event)
                        .await
                        .unwrap_or_else(|err| log_error!("error handling child process stdio (cli term relay): `{err}`"));
                    Ok(())
                })
            }),
        );

        Ok(())
    }

    fn register_cli_handlers(&self) -> Result<()> {
        self.cli.register_handlers()?;

        self.cli.handlers().register(&self.cli, crate::modules::metrics::Metrics::new(&self.core));

        Ok(())
    }

    fn register_window_handlers(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let close = callback!(move || {
            let this = this.clone();
            spawn(async move {
                this.cli.shutdown().await.unwrap_or_else(|err| log_error!("Error during shutdown: `{err}`"));
            });
        });

        self.window.on("close", close.as_ref());
        self.callbacks.retain(close)?;

        Ok(())
    }

    async fn main(self: &Arc<Self>) -> Result<()> {
        self.register_window_handlers()?;
        self.register_ipc_handlers()?;
        self.register_cli_handlers()?;
        crate::modules::register_cli_handlers(&self.cli)?;

        // cli.handlers().register(&cli, crate::modules::test::Test::default());

        // cli starts notification->term trace pipe task
        self.cli.start().await?;

        self.core.terminal_ready().await?;

        let kos_current_version = env!("CARGO_PKG_VERSION").to_string();
        let kos_last_version = self.settings.get::<String>(TerminalSettings::Greeting).unwrap_or_default();

        if kos_last_version != kos_current_version {
            let greeting = r"
Hello Kaspian!

If you have any questions, please join us on discord at https://discord.gg/kaspa
    
            ";

            self.cli.term().writeln(greeting.crlf());
            self.settings.set(TerminalSettings::Greeting, &kos_current_version).await?;
        }
        let framework_version = self.cli.version();
        let version = if framework_version == kos_current_version {
            kos_current_version
        } else {
            format!("{} Rust Core v{}", kos_current_version, framework_version)
        };
        let banner = format!("Kaspa OS v{} (type 'help' for list of commands)", version);
        self.cli.term().writeln(banner);

        // terminal blocks async execution, delivering commands to the cli
        self.cli.run().await?;

        // stop notification->term trace pipe task
        self.cli.stop().await?;

        self.core.shutdown().await?;

        self.window.close_impl(true);

        Ok(())
    }
}

#[wasm_bindgen]
pub async fn init_application() -> Result<()> {
    kaspa_core::log::set_log_level(LevelFilter::Info);
    workflow_log::set_colors_enabled(true);

    let terminal = Terminal::try_new().await?;
    terminal.main().await?;

    Ok(())
}
