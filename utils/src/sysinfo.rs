use crate::fd_budget;
use crate::hex::FromHex;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct SystemInfo {
    pub system_id: Option<Vec<u8>>,
    pub git_hash: Option<Vec<u8>>,
    pub cpu_physical_cores: u16,
    pub total_memory: u64,
    pub fd_limit: u32,
}

static SYSTEM_INFO: OnceLock<SystemInfo> = OnceLock::new();

const GIT_HASH: &str = env!("RUSTY_KASPA_GIT_COMMIT_HASH");

impl Default for SystemInfo {
    fn default() -> Self {
        let system_info = SYSTEM_INFO.get_or_init(|| {
            let mut system = sysinfo::System::new();
            system.refresh_memory();
            let cpu_physical_cores = num_cpus::get() as u16;
            let total_memory = system.total_memory();
            let fd_limit = fd_budget::limit() as u32;
            let system_id = Self::try_system_id();
            let git_hash = Self::try_git_hash_as_vec();

            SystemInfo { system_id, git_hash, cpu_physical_cores, total_memory, fd_limit }
        });
        (*system_info).clone()
    }
}

impl SystemInfo {
    /// Obtain a unique system (machine) identifier.
    fn try_system_id() -> Option<Vec<u8>> {
        let some_id = if let Ok(mut file) = File::open("/etc/machine-id") {
            // fetch the system id from /etc/machine-id
            let mut machine_id = String::new();
            file.read_to_string(&mut machine_id).ok();
            machine_id
        } else if let Ok(Some(mac)) = mac_address::get_mac_address() {
            // fallback on the mac address
            mac.to_string()
        } else {
            // 🤷
            return None;
        };
        let mut sha256 = Sha256::default();
        sha256.update(some_id.as_bytes());
        Some(sha256.finalize().to_vec())
    }

    /// Check if the codebase is built under a Git repository
    /// and return the hash of the current commit as `Vec<u8>`.
    fn try_git_hash_as_vec() -> Option<Vec<u8>> {
        FromHex::from_hex(GIT_HASH).ok()
    }
}

impl AsRef<SystemInfo> for SystemInfo {
    fn as_ref(&self) -> &SystemInfo {
        self
    }
}
