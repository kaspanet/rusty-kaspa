use crate::imports::*;
use kaspa_cli::metrics::MetricsData;

static mut METRICS: Option<Arc<Metrics>> = None;

#[derive(Clone)]
pub struct Metrics {
    pub inner: Arc<Application>,
    pub ipc: Arc<Ipc<MetricsOps>>,
    pub core: Arc<CoreIpc>,
    pub window: Arc<Window>,
    pub callbacks: CallbackMap,
    // pub shutdown: Arc<AtomicBool>,
    pub settings: Arc<SettingsStore<MetricsSettings>>,
    pub layout: Arc<Layout<SettingsStore<MetricsSettings>>>,
}

impl Metrics {
    pub async fn try_new() -> Result<Arc<Self>> {
        let core_ipc_target = get_ipc_target(Modules::Core).await?.expect("Unable to aquire background window");
        let core = Arc::new(CoreIpc::new(core_ipc_target));

        let settings = Arc::new(SettingsStore::<MetricsSettings>::try_new("metrics.settings")?);
        settings.try_load().await?;
        // - TODO -
        let _default_duration = settings.get::<f64>(MetricsSettings::Duration);

        let window = Arc::new(nw_sys::window::get());

        let layout = Arc::new(Layout::try_new(&window, &settings).await?);

        // - TODO - INJECT D3 via d3::load()..

        let app = Arc::new(Self {
            inner: Application::new()?,
            ipc: Ipc::try_new_window_binding(&window, Modules::Metrics)?,
            core,
            window,
            callbacks: CallbackMap::default(),
            // shutdown: Arc::new(AtomicBool::new(false)),
            settings,
            layout,
        });

        unsafe {
            METRICS = Some(app.clone());
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

                    let window = this.window.window();
                    let document = window.document().unwrap();
                    let body = document.query_selector("body").unwrap().ok_or_else(|| "Unable to get body element".to_string())?;

                    body.append_with_str_1(format!("{:?}", data).as_str()).unwrap_or_else(|e| {
                        log_error!("Unable to append metrics data: {:?}", e);
                    });

                    Ok(())
                })
            }),
        );

        Ok(())
    }

    fn register_window_handlers(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        let close_window = callback!(move || {
            let this = this.clone();
            spawn(async move {
                this.core.metrics_close().await.expect("Unable to close metrics");
            });
        });
        self.window.on("close", close_window.as_ref());
        self.callbacks.retain(close_window)?;

        Ok(())
    }

    async fn main(self: &Arc<Self>) -> Result<()> {
        self.register_window_handlers()?;
        self.register_ipc_handlers()?;

        // this call reflects from core to terminal
        // initiating metrica data relay
        self.core.metrics_active().await?;

        //        self.window.close_impl(true);

        Ok(())
    }
}

#[wasm_bindgen]
pub async fn init_metrics() -> Result<()> {
    kaspa_core::log::set_log_level(LevelFilter::Info);
    workflow_log::set_colors_enabled(true);

    let terminal = Metrics::try_new().await?;
    terminal.main().await?;

    Ok(())
}
