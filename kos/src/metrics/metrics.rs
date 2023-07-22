use crate::imports::*;

static mut APP: Option<Arc<App>> = None;

#[derive(Clone)]
pub struct App {
    pub inner: Arc<Application>,
    pub ipc: Arc<Ipc<MetricsOps>>,
    pub core: Arc<CoreIpc>,
    pub window: Arc<Window>,
    pub callbacks: CallbackMap,
    // pub shutdown: Arc<AtomicBool>,
    pub settings: SettingsStore<MetricsSettings>,
}

impl App {
    pub async fn try_new() -> Result<Arc<Self>> {
        let core_ipc_target = get_ipc_target(Modules::Core).await?.expect("Unable to aquire background window");
        let core = Arc::new(CoreIpc::new(core_ipc_target));

        let settings = SettingsStore::<MetricsSettings>::try_new("metrics.settings")?;
        settings.try_load().await?;
        // - TODO -
        let _default_duration = settings.get::<f64>(MetricsSettings::Duration);

        let window = Arc::new(nw_sys::window::get());

        // - TODO - INJECT D3 via d3::load()..

        let app = Arc::new(Self {
            inner: Application::new()?,
            ipc: Ipc::try_new_window_binding(&window, Modules::Metrics)?,
            core,
            window,
            callbacks: CallbackMap::default(),
            // shutdown: Arc::new(AtomicBool::new(false)),
            settings,
        });

        unsafe {
            APP = Some(app.clone());
        };

        Ok(app)
    }

    fn register_ipc_handlers(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        self.ipc.notification(
            MetricsOps::MetricsData,
            Notification::new(move |data: MetricsData| {
                let this = this.clone();
                Box::pin(async move {
                    log_info!("Received metrics data: {:?}", data);

                    Ok(())
                })
            }),
        );

        Ok(())
    }

    fn register_window_handlers(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let close = callback!(move || {
            let this = this.clone();
            // spawn(async move {
            //     this.cli.shutdown().await.unwrap_or_else(|err| log_error!("Error during shutdown: `{err}`"));
            // });
        });

        self.window.on("close", close.as_ref());
        self.callbacks.retain(close)?;

        Ok(())
    }

    async fn main(self: &Arc<Self>) -> Result<()> {
        self.register_window_handlers()?;
        self.register_ipc_handlers()?;

        //        self.window.close_impl(true);

        Ok(())
    }
}

#[wasm_bindgen]
pub async fn init_application() -> Result<()> {
    kaspa_core::log::set_log_level(LevelFilter::Info);
    workflow_log::set_colors_enabled(true);

    let terminal = App::try_new().await?;
    terminal.main().await?;

    Ok(())
}
