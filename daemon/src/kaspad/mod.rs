pub mod native;
pub mod wasm;

use crate::imports::*;

use wasm::{Process, ProcessEvent, ProcessOptions};
// use workflow_log::color_log::downcast_sync;

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct KaspadConfig {
    pub mute: bool,
    pub path: Option<String>,
    pub network: Option<NetworkType>,
    pub utxo_index: bool,
    // --- TODO: these are not used yet ---
    pub peers: Vec<String>,
    pub enablge_grpc: bool,
    pub enablge_borsh_rpc: bool,
    pub enablge_json_rpc: bool,
    pub is_grpc_public: bool,
    pub is_borsh_rpc_public: bool,
    pub is_json_rpc_public: bool,
    pub unsafe_rpc: bool,
    pub inbound_limit: Option<usize>,
    pub outbound_target: Option<usize>,
    pub async_threads: Option<usize>,
    pub no_logfiles: bool,
    // ---
}

impl KaspadConfig {
    pub fn new(path: &str, network: NetworkType, mute: bool) -> Result<Self> {
        Ok(Self { path: Some(path.to_string()), network: Some(network), mute, ..Default::default() })
    }
}

impl Default for KaspadConfig {
    fn default() -> Self {
        Self {
            mute: false,
            path: None,
            network: None,
            utxo_index: true,
            enablge_grpc: true,
            is_grpc_public: false,
            enablge_borsh_rpc: true,
            is_borsh_rpc_public: false,
            enablge_json_rpc: false,
            is_json_rpc_public: false,
            // --- TODO: these are not used yet ---
            peers: vec![],
            unsafe_rpc: false,
            inbound_limit: None,
            outbound_target: None,
            async_threads: None,
            no_logfiles: false,
            // ---
        }
    }
}

impl TryFrom<KaspadConfig> for Vec<String> {
    type Error = Error;
    fn try_from(args: KaspadConfig) -> Result<Vec<String>> {
        let mut argv = Vec::new();

        if args.path.is_none() {
            return Err(Error::Custom("no kaspad path is specified".to_string()));
        }

        if args.network.is_none() {
            return Err(Error::Custom("no network type is specified".to_string()));
        }

        // ---

        let binary_path = args.path.unwrap();
        argv.push(binary_path.as_str());

        let network = args.network.unwrap();

        match network {
            NetworkType::Mainnet => {}
            NetworkType::Testnet => {
                argv.push("--testnet");
            }
            NetworkType::Devnet => {
                argv.push("--devnet");
            }
            NetworkType::Simnet => {
                argv.push("--simnet");
            }
        }

        if args.utxo_index {
            argv.push("--utxoindex");
        }

        // ---

        if args.enablge_borsh_rpc {
            if args.is_borsh_rpc_public {
                argv.push("--rpclisten-borsh=public");
            } else {
                argv.push("--rpclisten-borsh=default");
            }
        }

        // ---

        if args.enablge_json_rpc {
            if args.is_borsh_rpc_public {
                argv.push("--rpclisten-borsh=public");
            } else {
                argv.push("--rpclisten-borsh=default");
            }
        }

        // ---

        let grpc_port = network.default_rpc_port();
        let grpc_listen_flag = if args.is_grpc_public {
            format!("--rpclisten=0.0.0.0:{}", grpc_port)
        } else {
            format!("--rpclisten=127.0.0.1:{}", grpc_port)
        };
        if args.enablge_grpc {
            argv.push(grpc_listen_flag.as_str());
        }

        Ok(argv.into_iter().map(String::from).collect())
    }
}

struct Inner {
    process: Option<Arc<Process>>,
    config: Mutex<KaspadConfig>,
}

impl Default for Inner {
    fn default() -> Self {
        Self { process: None, config: Mutex::new(Default::default()) }
    }
}

pub struct Kaspad {
    inner: Arc<Mutex<Inner>>,
    mute: Arc<AtomicBool>,
    events: Channel<ProcessEvent>,
}

impl Default for Kaspad {
    fn default() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())), mute: Arc::new(AtomicBool::new(false)), events: Channel::unbounded() }
    }
}

impl Kaspad {
    pub fn new(args: KaspadConfig) -> Self {
        Self {
            mute: Arc::new(AtomicBool::new(args.mute)),
            inner: Arc::new(Mutex::new(Inner { config: Mutex::new(args), ..Default::default() })),
            events: Channel::unbounded(),
        }
    }

    pub fn configure(&self, config: KaspadConfig) -> Result<()> {
        self.mute.store(config.mute, Ordering::SeqCst);
        *self.inner().config.lock().unwrap() = config;
        Ok(())
    }

    fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn uptime(&self) -> Option<Duration> {
        if let Some(process) = self.inner().process.as_ref() {
            process.uptime()
        } else {
            None
        }
    }

    pub fn process(&self) -> Option<Arc<Process>> {
        self.inner().process.clone()
    }

    pub fn events(&self) -> &Channel<ProcessEvent> {
        &self.events
    }

    pub fn try_argv(&self) -> Result<Vec<String>> {
        self.inner().config.lock().unwrap().clone().try_into()
    }

    pub fn start(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            if process.is_running() {
                return Err(Error::Custom("Kaspa node is already running.".to_string()));
            }
        }

        let argv = self.try_argv()?;
        let argv = argv.iter().map(String::as_str).collect::<Vec<_>>();
        let cwd = PathBuf::from(nw_sys::app::folder());

        let options = ProcessOptions::new(
            argv.as_slice(),
            Some(cwd),
            true,
            Some(Duration::from_millis(1_000)),
            true,
            // Some(Duration::from_millis(45_000)),
            Some(Duration::from_millis(5_000)),
            self.events().clone(),
            Some(64),
            self.mute.load(Ordering::SeqCst),
        );

        // let options = KaspadOptions::new(path,network)?;
        let process = Arc::new(Process::new(options));
        self.inner().process.replace(process.clone());
        process.run()?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        if let Some(process) = self.process() {
            process.stop()?;
        }
        Ok(())
    }

    pub fn restart(&self) -> Result<()> {
        if let Some(process) = self.process() {
            let argv = self.try_argv()?;
            process.replace_argv(argv);
            process.restart()?;
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<()> {
        if let Some(process) = self.process() {
            process.kill()?;
        }
        Ok(())
    }

    pub async fn kill_and_join(&self) -> Result<()> {
        if let Some(process) = self.process() {
            process.kill()?;
            process.join().await?;
        }
        Ok(())
    }

    pub async fn stop_and_join(&self) -> Result<()> {
        if let Some(process) = self.process() {
            process.stop_and_join().await?;
        }
        Ok(())
    }

    pub async fn join(&self) -> Result<()> {
        if let Some(process) = self.process() {
            process.join().await?;
        }
        Ok(())
    }

    pub async fn mute(&self, mute: bool) -> Result<()> {
        if let Some(process) = self.process() {
            process.mute(mute)?;
            self.mute.store(mute, Ordering::SeqCst);
        }
        Ok(())
    }

    pub async fn toggle_mute(&self) -> Result<()> {
        if let Some(process) = self.process() {
            process.toggle_mute()?;
        }
        Ok(())
    }

    pub async fn version(&self) -> Result<String> {
        let path = self.inner().config.lock().unwrap().path.clone();
        if let Some(path) = path {
            Ok(Process::version(path.as_str()).await?.to_string())
        } else {
            Ok("Kaspad binary is not configured. Please use 'node select' command.".to_string())
        }
    }
}

#[async_trait]
pub trait KaspadCtl {
    async fn version(&self) -> Result<String>;
    async fn configure(&self, config: KaspadConfig) -> Result<()>;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn join(&self) -> Result<()>;
    async fn restart(&self) -> Result<()>;
    async fn kill(&self) -> Result<()>;
    async fn status(&self) -> Result<DaemonStatus>;
    async fn mute(&self, mute: bool) -> Result<()>;
    async fn toggle_mute(&self) -> Result<()>;

    async fn is_running(&self) -> Result<bool> {
        Ok(self.status().await?.uptime.is_some())
    }
}
