pub mod native;
pub mod wasm;

use crate::imports::*;

use wasm::{Process, ProcessOptions};

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct CpuMinerConfig {
    pub path: Option<String>,
    pub network: Option<NetworkType>,
    pub threads: Option<usize>,
    pub throttle: Option<usize>,
    pub address: Option<Address>,
}

impl CpuMinerConfig {
    pub fn new(path: &str, network: NetworkType, address: Address) -> Result<Self> {
        Ok(Self { path: Some(path.to_string()), network: Some(network), address: Some(address), ..Default::default() })
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
            NetworkType::Mainnet => {}
            NetworkType::Testnet => {
                argv.push("--testnet");
            }
            _ => {
                return Err(Error::Custom("network type is not suported by the CPU miner".to_string()));
            }
        }

        if args.address.is_none() {
            return Err(Error::Custom("no address is specified".to_string()));
        }

        let threads = args.threads.unwrap_or(1);
        let threads = format!("--threads={threads}");
        argv.push(threads.as_str());

        let throttle = args.throttle.unwrap_or(3);
        let throttle = format!("--throttle={throttle}");
        argv.push(throttle.as_str());

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
    stdout: Channel<String>,
    stderr: Channel<String>,
}

impl Default for CpuMiner {
    fn default() -> Self {
        Self { inner: Arc::new(Mutex::new(Inner::default())), stdout: Channel::unbounded(), stderr: Channel::unbounded() }
    }
}

impl CpuMiner {
    pub fn new(args: CpuMinerConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner { config: Mutex::new(args), ..Default::default() })),
            stdout: Channel::unbounded(),
            stderr: Channel::unbounded(),
        }
    }

    pub fn configure(&self, config: CpuMinerConfig) -> Result<()> {
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

    pub fn stdout(&self) -> &Channel<String> {
        &self.stdout
    }

    pub fn stderr(&self) -> &Channel<String> {
        &self.stderr
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
            Some(self.stdout().clone()),
            Some(self.stderr().clone()),
        );

        // let options = KaspadOptions::new(path,network)?;
        let process = Arc::new(Process::new(options));
        self.inner().process.replace(process.clone());
        process.run()?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            process.stop()?;
        }
        Ok(())
    }

    pub fn restart(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            let argv = self.try_argv()?;
            process.replace_argv(argv);
            process.restart()?;
        }
        Ok(())
    }

    pub fn kill(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            process.kill()?;
        }
        Ok(())
    }

    pub async fn kill_and_join(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            process.kill()?;
            process.join().await?;
        }
        Ok(())
    }

    pub async fn stop_and_join(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            process.stop_and_join().await?;
        }
        Ok(())
    }
}

#[async_trait]
pub trait CpuMinerCtl {
    async fn configure(&self, config: CpuMinerConfig) -> Result<()>;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn restart(&self) -> Result<()>;
    async fn kill(&self) -> Result<()>;
    async fn status(&self) -> Result<DaemonStatus>;
}
