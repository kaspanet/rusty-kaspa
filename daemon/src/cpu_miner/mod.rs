pub mod native;
pub mod wasm;

use crate::imports::*;

use wasm::{version, Process, ProcessEvent, ProcessOptions};

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct CpuMinerConfig {
    pub mute: bool,
    pub path: Option<String>,
    pub network: Option<NetworkType>,
    pub address: Option<Address>,
    pub server: Option<String>,
    pub threads: Option<usize>,
    pub throttle: Option<usize>,
}

impl CpuMinerConfig {
    pub fn new(path: &str, network: NetworkType, address: Address, server: String, throttle: usize, mute: bool) -> Self {
        Self {
            mute,
            path: Some(path.to_string()),
            network: Some(network),
            address: Some(address),
            server: Some(server),
            throttle: Some(throttle),
            ..Default::default()
        }
    }

    pub fn new_for_version(path: &str) -> Self {
        Self { path: Some(path.to_string()), ..Default::default() }
    }
}

impl TryFrom<CpuMinerConfig> for Vec<String> {
    type Error = Error;
    fn try_from(args: CpuMinerConfig) -> Result<Vec<String>> {
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
            NetworkType::Mainnet => {
                argv.push("--port=16110");
            }
            NetworkType::Testnet => {
                argv.push("--testnet");
                argv.push("--port=16210");
            }
            _ => {
                return Err(Error::Custom("network type is not suported by the CPU miner".to_string()));
            }
        }

        let server = args.server.unwrap_or("127.0.0.1".to_string());
        let server = format!("--kaspad-address={server}");
        argv.push(server.as_str());

        if args.address.is_none() {
            return Err(Error::Custom("no address is specified".to_string()));
        }
        let address = args.address.unwrap();
        let address = format!("--mining-address={address}");
        argv.push(address.as_str());

        let threads = args.threads.unwrap_or(1);
        let threads = format!("--threads={threads}");
        argv.push(threads.as_str());

        let throttle = args.throttle.unwrap_or(5_000);
        let throttle = format!("--throttle={throttle}");
        argv.push(throttle.as_str());

        argv.push("--altlogs");

        Ok(argv.into_iter().map(String::from).collect())
    }
}

struct Inner {
    process: Option<Arc<Process>>,
    config: Mutex<CpuMinerConfig>,
}

impl Default for Inner {
    fn default() -> Self {
        Self { process: None, config: Mutex::new(Default::default()) }
    }
}

pub struct CpuMiner {
    inner: Arc<Mutex<Inner>>,
    mute: Arc<AtomicBool>,
    events: Channel<ProcessEvent>,
}

impl Default for CpuMiner {
    fn default() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())), events: Channel::unbounded(), mute: Arc::new(AtomicBool::new(false)) }
    }
}

impl CpuMiner {
    pub fn new(args: CpuMinerConfig) -> Self {
        Self {
            mute: Arc::new(AtomicBool::new(args.mute)),
            inner: Arc::new(Mutex::new(Inner { config: Mutex::new(args), ..Default::default() })),
            events: Channel::unbounded(),
        }
    }

    pub fn configure(&self, config: CpuMinerConfig) -> Result<()> {
        self.mute.store(config.mute, Ordering::SeqCst);
        *self.inner().config.lock().unwrap() = config;
        Ok(())
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
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
                return Err(Error::Custom("Miner is already running.".to_string()));
            }
        }

        let argv = self.try_argv()?;
        let argv = argv.iter().map(String::as_str).collect::<Vec<_>>();
        let cwd = PathBuf::from(nw_sys::app::start_path());

        let options = ProcessOptions::new(
            argv.as_slice(),
            Some(cwd),
            true,
            Some(Duration::from_millis(1000)),
            false,
            None,
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
            Ok(version(path.as_str()).await?.to_string())
        } else {
            Err("miner binary is not configured (please use 'miner select' command.".into())
        }
    }
}

#[async_trait]
pub trait CpuMinerCtl {
    async fn version(&self) -> Result<String>;
    async fn configure(&self, config: CpuMinerConfig) -> Result<()>;
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
