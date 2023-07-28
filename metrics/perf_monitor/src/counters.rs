use portable_atomic::{AtomicF64, AtomicUsize};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub(crate) struct Counters {
    /// this is the non-swapped physical memory a process has used.
    /// On UNIX it matches `top`'s RES column.
    ///
    /// On Windows this is an alias for wset field and it matches "Mem Usage"
    /// column of taskmgr.exe.
    pub resident_set_size: AtomicU64,
    /// this is the total amount of virtual memory used by the process.
    /// On UNIX it matches `top`'s VIRT column.
    ///
    /// On Windows this is an alias for pagefile field and it matches "Mem
    /// Usage" "VM Size" column of taskmgr.exe.
    pub virtual_memory_size: AtomicU64,

    pub core_num: AtomicUsize,
    pub cpu_usage: AtomicF64,

    pub fd_num: AtomicUsize,

    pub disk_io_read_bytes: AtomicU64,
    pub disk_io_write_bytes: AtomicU64,
    pub disk_io_read_per_sec: AtomicF64,
    pub disk_io_write_per_sec: AtomicF64,
}

impl Counters {
    pub(crate) fn update(&self, snapshot: CountersSnapshot) {
        self.resident_set_size.store(snapshot.resident_set_size, Ordering::Release);
        self.virtual_memory_size.store(snapshot.resident_set_size, Ordering::Release);
        self.core_num.store(snapshot.core_num, Ordering::Release);
        self.cpu_usage.store(snapshot.cpu_usage, Ordering::Release);
        self.fd_num.store(snapshot.fd_num, Ordering::Release);
        self.disk_io_read_bytes.store(snapshot.disk_io_read_bytes, Ordering::Release);
        self.disk_io_write_bytes.store(snapshot.disk_io_write_bytes, Ordering::Release);
        self.disk_io_read_per_sec.store(snapshot.disk_io_read_per_sec, Ordering::Release);
        self.disk_io_write_per_sec.store(snapshot.disk_io_write_per_sec, Ordering::Release);
    }
    pub(crate) fn snapshot(&self) -> CountersSnapshot {
        CountersSnapshot {
            resident_set_size: self.resident_set_size.load(Ordering::Acquire),
            virtual_memory_size: self.virtual_memory_size.load(Ordering::Acquire),
            core_num: self.core_num.load(Ordering::Acquire),
            cpu_usage: self.cpu_usage.load(Ordering::Acquire),
            fd_num: self.fd_num.load(Ordering::Acquire),
            disk_io_read_bytes: self.disk_io_read_bytes.load(Ordering::Acquire),
            disk_io_write_bytes: self.disk_io_write_bytes.load(Ordering::Acquire),
            disk_io_read_per_sec: self.disk_io_read_per_sec.load(Ordering::Acquire),
            disk_io_write_per_sec: self.disk_io_write_per_sec.load(Ordering::Acquire),
        }
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub struct CountersSnapshot {
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
