pub mod native;
pub mod wasm;

use crate::imports::*;

use wasm::{Process, ProcessOptions};

#[derive(Clone, Debug)]
pub struct Args {
    pub path: PathBuf,
    pub utxo_index: bool,
    pub network: NetworkType,
    // --- TODO: these are not used yet ---
    pub peers: Vec<String>,
    pub enablge_grpc: bool,
    pub enablge_json_rpc: bool,
    pub unsafe_rpc: bool,
    pub inbound_limit: Option<usize>,
    pub outbound_target: Option<usize>,
    pub async_threads: Option<usize>,
    pub no_logfiles: bool,
    // ---
}

impl Args {
    pub fn new(path: &Path, network: NetworkType) -> Result<Self> {
        Ok(Self {
            path: PathBuf::from(path),
            network,
            utxo_index: true,
            // --- TODO: these are not used yet ---
            peers: vec![],
            enablge_grpc: false,
            enablge_json_rpc: false,
            unsafe_rpc: false,
            inbound_limit: None,
            outbound_target: None,
            async_threads: None,
            no_logfiles: false,
            // ---
        })
    }
}

impl From<Args> for Vec<String> {
    fn from(ko: Args) -> Vec<String> {
        let mut argv = Vec::new();
        let binary_path = ko.path.to_string_lossy().to_string();
        argv.push(binary_path.as_str());

        match ko.network {
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

        if ko.utxo_index {
            argv.push("--utxoindex");
        }

        argv.push("--rpclisten-borsh=default");

        argv.into_iter().map(String::from).collect()
    }
}

pub struct Inner {
    process: Option<Arc<Process>>,
    stdout: Channel<String>,
    stderr: Channel<String>,
    options: Args,
}

pub struct Kaspad {
    inner: Arc<Mutex<Inner>>,
}

impl Kaspad {
    pub fn new(options: Args) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner { process: None, stdout: Channel::unbounded(), stderr: Channel::unbounded(), options })),
        }
    }

    pub fn inner(&self) -> MutexGuard<Inner> {
        self.inner.lock().unwrap()
    }

    pub fn process(&self) -> Option<Arc<Process>> {
        self.inner().process.clone()
    }

    pub fn stdout(&self) -> Channel<String> {
        self.inner().stdout.clone()
    }

    pub fn stderr(&self) -> Channel<String> {
        self.inner().stderr.clone()
    }

    pub fn argv(&self) -> Vec<String> {
        self.inner().options.clone().into()
    }

    pub fn start(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            if process.is_running() {
                return Err(Error::Custom("Kaspa node is already running.".to_string()));
            }
        }

        let argv = self.argv();
        let argv = argv.iter().map(String::as_str).collect::<Vec<_>>();
        let cwd = PathBuf::from(nw_sys::app::start_path());

        let options = ProcessOptions::new(
            argv.as_slice(),
            Some(cwd),
            true,
            Some(Duration::from_millis(1000)),
            false,
            None,
            Some(self.stdout()),
            Some(self.stderr()),
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
            process.replace_argv(self.argv());
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

    pub async fn shutdown(&self) -> Result<()> {
        let process = self.process();
        if let Some(process) = process {
            process.stop_and_join().await?;
        }
        Ok(())
    }
}
