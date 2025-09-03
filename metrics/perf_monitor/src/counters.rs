use portable_atomic::{AtomicF64, AtomicUsize};
use std::{
    fmt::Display,
    sync::atomic::{AtomicU64, Ordering},
};

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
        self.virtual_memory_size.store(snapshot.virtual_memory_size, Ordering::Release);
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

impl CountersSnapshot {
    pub fn to_process_metrics_display(&self) -> ProcessMetricsDisplay<'_> {
        ProcessMetricsDisplay(self)
    }

    pub fn to_io_metrics_display(&self) -> IoMetricsDisplay<'_> {
        IoMetricsDisplay(self)
    }
}

fn to_human_readable(mut number_to_format: f64, precision: usize, suffix: &str) -> String {
    const UNITS: [&str; 7] = ["", "K", "M", "G", "T", "P", "E"];
    const DIV: [f64; 7] =
        [1.0, 1_000.0, 1_000_000.0, 1_000_000_000.0, 1_000_000_000_000.0, 1_000_000_000_000_000.0, 1_000_000_000_000_000_000.0];
    let i = (number_to_format.log(1000.0) as usize).min(UNITS.len() - 1);
    number_to_format /= DIV[i];
    format!("{number_to_format:.precision$}{}{}", UNITS[i], suffix)
}

pub struct ProcessMetricsDisplay<'a>(&'a CountersSnapshot);

pub struct IoMetricsDisplay<'a>(&'a CountersSnapshot);

impl Display for ProcessMetricsDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "process metrics: RAM: {} ({}), VIRT: {} ({}), FD: {}, cores: {}, total cpu usage: {:.4}",
            self.0.resident_set_size,
            to_human_readable(self.0.resident_set_size as f64, 2, "B"),
            self.0.virtual_memory_size,
            to_human_readable(self.0.virtual_memory_size as f64, 2, "B"),
            self.0.fd_num,
            self.0.core_num,
            self.0.cpu_usage,
        )
    }
}

impl Display for IoMetricsDisplay<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "disk io metrics: read: {} ({}), write: {} ({}), read rate: {:.3} ({}), write rate: {:.3} ({})",
            self.0.disk_io_read_bytes,
            to_human_readable(self.0.disk_io_read_bytes as f64, 0, "B"),
            self.0.disk_io_write_bytes,
            to_human_readable(self.0.disk_io_write_bytes as f64, 0, "B"),
            self.0.disk_io_read_per_sec,
            to_human_readable(self.0.disk_io_read_per_sec, 0, "B/s"),
            self.0.disk_io_write_per_sec,
            to_human_readable(self.0.disk_io_write_per_sec, 0, "B/s")
        )
    }
}
