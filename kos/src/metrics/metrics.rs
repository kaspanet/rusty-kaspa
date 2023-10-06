use super::toolbar::*;
use crate::imports::*;
use kaspa_cli_lib::metrics::{Metric, MetricsSnapshot};
use std::collections::HashMap;
use workflow_core::time::{HOURS, MINUTES};
use workflow_d3::container::*;
use workflow_d3::graph::*;

static mut METRICS: Option<Arc<Metrics>> = None;

#[derive(Clone)]
pub struct Metrics {
    pub inner: Arc<Application>,
    pub ipc: Arc<Ipc<MetricsOps>>,
    pub core: Arc<CoreIpc>,
    pub window: Arc<Window>,
    pub callbacks: CallbackMap,
    pub settings: Arc<SettingsStore<MetricsSettings>>,
    pub layout: Arc<Layout<SettingsStore<MetricsSettings>>>,
    pub container: Arc<Mutex<Option<Arc<Container>>>>,
    pub graphs: Arc<Mutex<HashMap<Metric, Arc<Graph>>>>,
    pub toolbar: Toolbar,
}

impl Metrics {
    pub async fn try_new() -> Result<Arc<Self>> {
        workflow_d3::load().await?;

        let core_ipc_target = get_ipc_target(Modules::Core).await?.expect("Unable to aquire background window");
        let core = Arc::new(CoreIpc::new(core_ipc_target));

        let settings = Arc::new(SettingsStore::<MetricsSettings>::try_new("metrics")?);
        settings.try_load().await?;
        // - TODO - setup graph time duration
        let _default_duration = settings.get::<f64>(MetricsSettings::Duration);

        let window = Arc::new(nw_sys::window::get());

        let layout = Arc::new(Layout::try_new(&window, &settings).await?);
        let container = Arc::new(Mutex::new(None));
        let graphs = Arc::new(Mutex::new(HashMap::new()));
        let toolbar = Toolbar::try_new(&window.window(), &container, &graphs)?;
        toolbar.try_init()?;
        let app = Arc::new(Self {
            inner: Application::new()?,
            ipc: Ipc::try_new_window_binding(&window, Modules::Metrics)?,
            core,
            window,
            callbacks: CallbackMap::default(),
            // shutdown: Arc::new(AtomicBool::new(false)),
            settings,
            layout,
            container,
            graphs,
            toolbar,
        });

        unsafe {
            METRICS = Some(app.clone());
        };

        Ok(app)
    }

    fn register_ipc_handlers(self: &Arc<Self>) -> Result<()> {
        let this = self.clone();
        self.ipc.notification(
            MetricsOps::MetricsSnapshot,
            Notification::new(move |data: MetricsSnapshot| {
                let this = this.clone();
                Box::pin(async move {
                    // log_info!("Received metrics data: {:?}", data);
                    this.ingest(data).await?;
                    yield_executor().await;
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

    pub fn theme(name: &str, kind: &str) -> Box<GraphThemeOptions> {
        let font = "'Consolas', 'Lucida Grande', 'Roboto Mono', 'Source Code Pro', 'Trebuchet'";

        let primary = match name {
            "light" => "black",
            "dark" => "#ccc",
            _ => "grey",
        };

        let theme = match kind {
            "kaspa" => GraphThemeOptions::new(font, primary, "rgb(220, 240, 231)", "rgb(17, 187, 125)", primary),
            _ => GraphThemeOptions::new(font, primary, "rgb(220, 231, 240)", "rgb(17, 125, 187)", primary),
        };

        Box::new(theme)
    }

    async fn init_graphs(self: &Arc<Self>) -> Result<()> {
        let window = self.window.window();

        Container::try_init().await?;
        Graph::try_init(Some("graph")).await?;

        let container = Arc::new(Container::try_new(&window).await?);
        *self.container.lock().unwrap() = Some(container.clone());
        let mut graphs = vec![];
        for metric in Metric::list() {
            let graph = Arc::new(
                Graph::try_new(
                    &self.window.window(),
                    &container,
                    None,
                    "",
                    Duration::from_millis(5 * MINUTES),
                    Duration::from_millis(48 * HOURS),
                    GraphTheme::Custom(Self::theme("light", metric.group())),
                    Margin::new(20.0, 20.0, 10.0, 30.0),
                )
                .await?,
            );
            self.graphs.lock().unwrap().insert(metric, graph.clone());
            graphs.push(graph);
        }

        Ok(())
    }

    fn graph(&self, metric: &Metric) -> Arc<Graph> {
        self.graphs.lock().unwrap().get(metric).cloned().expect("Unable to find graph")
    }

    async fn ingest(self: &Arc<Self>, data: MetricsSnapshot) -> Result<()> {
        let si = true;
        for metric in Metric::list() {
            let value = data.get(&metric);
            self.graph(&metric).ingest(data.unixtime, value, &data.format(&metric, si)).await?;
        }

        yield_executor().await;
        sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    async fn main(self: &Arc<Self>) -> Result<()> {
        self.register_window_handlers()?;
        self.register_ipc_handlers()?;

        self.init_graphs().await?;

        // this call reflects from core to terminal
        // initiating metrica data relay
        self.core.metrics_ready().await?;

        Ok(())
    }
}

#[wasm_bindgen]
pub async fn init_metrics() -> Result<()> {
    kaspa_core::log::set_log_level(LevelFilter::Info);
    workflow_log::set_colors_enabled(true);

    let metrics = Metrics::try_new().await.unwrap_or_else(|err| {
        panic!("Unable to initialize metrics: {:?}", err);
    });
    metrics.main().await?;

    Ok(())
}
