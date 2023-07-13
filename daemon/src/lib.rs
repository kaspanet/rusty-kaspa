pub mod error;
pub mod imports;
pub mod kaspad;
pub mod result;

use crate::imports::*;
pub use crate::result::Result;
pub use kaspad::{Kaspad, KaspadConfig, KaspadCtl};
use workflow_core::runtime;
use workflow_store::fs::*;

pub static LOCATIONS: &[&str] = &["bin", "../target/release", "../target/debug"];

pub async fn locate_binaries(root: &str, name: &str) -> Result<Vec<PathBuf>> {
    if !runtime::is_nw() && !runtime::is_node() && !runtime::is_native() {
        return Err(Error::Platform);
    }

    let name = if runtime::is_windows() { name.to_string() + ".exe" } else { name.to_string() };

    // let locations = LOCATIONS.iter().map(|path| absolute(&PathBuf::from(&root).join(path).join(&name)).map_err(|e|e.into())).collect::<Result<Vec<_>>>()?;
    let locations = LOCATIONS
        .iter()
        .map(|path| PathBuf::from(&root).join(path).join(&name).absolute().map_err(|e| e.into()))
        .collect::<Result<Vec<_>>>()?;

    let mut list = Vec::new();
    for path in locations {
        log_info!("locating binary: {}", path.display());
        if exists(&path).await? {
            log_info!("found binary: {}", path.display());
            list.push(path);
        } else {
            log_info!("did not find binary: {}", path.display());
        }
    }

    Ok(list)
}

pub enum DaemonKind {
    Kaspad,
    // MinerCpu,
}

#[derive(Default)]
pub struct Daemons {
    pub kaspad: Option<Arc<dyn KaspadCtl + Send + Sync + 'static>>,
}

impl Daemons {
    pub fn new() -> Self {
        Self { kaspad: None }
    }

    pub fn with_kaspad(mut self, kaspad: Arc<dyn KaspadCtl + Send + Sync + 'static>) -> Self {
        self.kaspad = Some(kaspad);
        self
    }
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum Stdio {
    Stdout(String),
    Stderr(String),
}

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub uptime: Option<u64>,
}
