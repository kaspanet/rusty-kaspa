#[derive(Debug, Default, Copy, Clone)]
pub struct Counters {
    /// this is the non-swapped physical memory a process has used.
    /// On UNIX it matches `top`'s RES column.
    ///
    /// On Windows this is an alias for wset field and it matches "Mem Usage"
    /// column of taskmgr.exe.
    pub resident_set_size: u64,
    /// this is the total amount of virtual memory used by the process.
    /// On UNIX it matches `top`'s VIRT column.
    ///
    /// On Windows this is an alias for pagefile field and it matches "Mem
    /// Usage" "VM Size" column of taskmgr.exe.
    pub virtual_memory_size: u64,

    pub core_num: usize,
    pub cpu_usage: f64,

    pub fd_num: usize,

    pub disk_io_read_bytes: u64,
    pub disk_io_write_bytes: u64,
    pub disk_io_read_per_sec: f64,
    pub disk_io_write_per_sec: f64,
}
