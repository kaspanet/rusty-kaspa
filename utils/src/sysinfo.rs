use crate::fd_budget;

pub struct SystemInfo {
    pub cpu_physical_cores: u16,
    pub total_memory: u64,
    pub fd_limit: u32,
}

impl Default for SystemInfo {
    fn default() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        let cpu_physical_cores = num_cpus::get() as u16;
        let total_memory = system.total_memory();

        let fd_limit = fd_budget::limit() as u32;

        SystemInfo { cpu_physical_cores, total_memory, fd_limit }
    }
}
