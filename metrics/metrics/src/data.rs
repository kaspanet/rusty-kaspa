use borsh::{BorshDeserialize, BorshSerialize};
use separator::{separated_float, separated_int, separated_uint_with_output, Separatable};
use serde::{Deserialize, Serialize};
use workflow_core::enums::Describe;

#[derive(Describe, Debug, Clone, Copy, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricGroup {
    System,
    Storage,
    Node,
    Network,
}

impl std::fmt::Display for MetricGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricGroup::System => write!(f, "system"),
            MetricGroup::Storage => write!(f, "storage"),
            MetricGroup::Node => write!(f, "node"),
            MetricGroup::Network => write!(f, "network"),
        }
    }
}

impl MetricGroup {
    pub fn title(&self) -> &str {
        match self {
            MetricGroup::System => "System",
            MetricGroup::Storage => "Storage",
            MetricGroup::Node => "Node",
            MetricGroup::Network => "Network",
        }
    }
}

impl MetricGroup {
    pub fn iter() -> impl Iterator<Item = MetricGroup> {
        [MetricGroup::System, MetricGroup::Storage, MetricGroup::Node, MetricGroup::Network].into_iter()
    }

    pub fn metrics(&self) -> impl Iterator<Item = Metric> {
        match self {
            MetricGroup::System => vec![Metric::CpuUsage, Metric::ResidentSetSizeBytes, Metric::VirtualMemorySizeBytes],
            MetricGroup::Storage => vec![
                Metric::FdNum,
                Metric::DiskIoReadBytes,
                Metric::DiskIoWriteBytes,
                Metric::DiskIoReadPerSec,
                Metric::DiskIoWritePerSec,
            ],
            MetricGroup::Node => vec![Metric::PeersConnected],
            MetricGroup::Network => vec![
                Metric::BlocksSubmitted,
                Metric::HeaderCount,
                Metric::DepCounts,
                Metric::BodyCounts,
                Metric::TxnCounts,
                Metric::Tps,
                Metric::ChainBlockCounts,
                Metric::MassCounts,
                Metric::BlockCount,
                Metric::TipHashes,
                Metric::Difficulty,
                Metric::PastMedianTime,
                Metric::VirtualParentHashes,
                Metric::VirtualDaaScore,
            ],
        }
        .into_iter()
    }
}

impl From<Metric> for MetricGroup {
    fn from(value: Metric) -> Self {
        match value {
            Metric::CpuUsage | Metric::ResidentSetSizeBytes | Metric::VirtualMemorySizeBytes => MetricGroup::System,
            // --
            Metric::FdNum
            | Metric::DiskIoReadBytes
            | Metric::DiskIoWriteBytes
            | Metric::DiskIoReadPerSec
            | Metric::DiskIoWritePerSec => MetricGroup::Storage,
            // --
            Metric::PeersConnected => MetricGroup::Node,
            // --
            Metric::BlocksSubmitted
            | Metric::HeaderCount
            | Metric::DepCounts
            | Metric::BodyCounts
            | Metric::TxnCounts
            | Metric::Tps
            | Metric::ChainBlockCounts
            | Metric::MassCounts
            | Metric::BlockCount
            | Metric::TipHashes
            | Metric::Difficulty
            | Metric::PastMedianTime
            | Metric::VirtualParentHashes
            | Metric::VirtualDaaScore => MetricGroup::Network,
        }
    }
}

#[derive(Describe, Debug, Clone, Copy, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum Metric {
    // CpuCores is used to normalize CpuUsage metric
    // CpuCores
    CpuUsage,
    ResidentSetSizeBytes,
    VirtualMemorySizeBytes,
    // ---
    FdNum,
    DiskIoReadBytes,
    DiskIoWriteBytes,
    DiskIoReadPerSec,
    DiskIoWritePerSec,
    // ---
    PeersConnected,
    // ---
    BlocksSubmitted,
    HeaderCount,
    DepCounts,
    BodyCounts,
    TxnCounts,
    Tps,
    ChainBlockCounts,
    MassCounts,
    BlockCount,
    TipHashes,
    Difficulty,
    PastMedianTime,
    VirtualParentHashes,
    VirtualDaaScore,
}

impl Metric {
    pub fn group(&self) -> &'static str {
        match self {
            Metric::CpuUsage
            | Metric::ResidentSetSizeBytes
            | Metric::VirtualMemorySizeBytes
            | Metric::FdNum
            | Metric::DiskIoReadBytes
            | Metric::DiskIoWriteBytes
            | Metric::DiskIoReadPerSec
            | Metric::DiskIoWritePerSec => "system",
            // --
            Metric::PeersConnected
            | Metric::BlocksSubmitted
            | Metric::HeaderCount
            | Metric::DepCounts
            | Metric::BodyCounts
            | Metric::TxnCounts
            | Metric::Tps
            | Metric::ChainBlockCounts
            | Metric::MassCounts
            | Metric::BlockCount
            | Metric::TipHashes
            | Metric::Difficulty
            | Metric::PastMedianTime
            | Metric::VirtualParentHashes
            | Metric::VirtualDaaScore => "kaspa",
        }
    }

    pub fn format(&self, f: f64, si: bool, short: bool) -> String {
        match self {
            Metric::CpuUsage => format!("{:1.2}%", f),
            Metric::ResidentSetSizeBytes => as_mb(f, si, short),
            Metric::VirtualMemorySizeBytes => as_mb(f, si, short),
            Metric::FdNum => f.separated_string(),
            // --
            Metric::DiskIoReadBytes => as_mb(f, si, short),
            Metric::DiskIoWriteBytes => as_mb(f, si, short),
            Metric::DiskIoReadPerSec => format!("{}/s", as_kb(f, si, short)),
            Metric::DiskIoWritePerSec => format!("{}/s", as_kb(f, si, short)),
            // --
            Metric::PeersConnected => f.separated_string(),
            // --
            Metric::BlocksSubmitted => format_as_float(f, short),
            Metric::HeaderCount => format_as_float(f, short),
            Metric::DepCounts => format_as_float(f, short),
            Metric::BodyCounts => format_as_float(f, short),
            Metric::TxnCounts => format_as_float(f, short),
            Metric::Tps => format_as_float(f.trunc(), short),
            Metric::ChainBlockCounts => format_as_float(f, short),
            Metric::MassCounts => format_as_float(f, short),
            Metric::BlockCount => format_as_float(f, short),
            Metric::TipHashes => format_as_float(f, short),
            Metric::Difficulty => format_as_float(f, short),
            Metric::PastMedianTime => format_as_float(f, short),
            Metric::VirtualParentHashes => format_as_float(f, short),
            Metric::VirtualDaaScore => format_as_float(f, short),
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Metric::CpuUsage => "CPU",
            Metric::ResidentSetSizeBytes => "Resident Memory",
            Metric::VirtualMemorySizeBytes => "Virtual Memory",
            // --
            Metric::FdNum => "File Handles",
            Metric::DiskIoReadBytes => "Storage Read",
            Metric::DiskIoWriteBytes => "Storage Write",
            Metric::DiskIoReadPerSec => "Storage Read",
            Metric::DiskIoWritePerSec => "Storage Write",
            // --
            Metric::PeersConnected => "Peers Connected",
            Metric::BlocksSubmitted => "Blocks Submitted",
            Metric::HeaderCount => "Headers",
            Metric::DepCounts => "Dependencies",
            Metric::BodyCounts => "Body Counts",
            Metric::TxnCounts => "Transactions",
            Metric::Tps => "TPS",
            Metric::ChainBlockCounts => "Chain Blocks",
            Metric::MassCounts => "Mass Counts",
            Metric::BlockCount => "Blocks",
            Metric::TipHashes => "Tip Hashes",
            Metric::Difficulty => "Difficulty",
            Metric::PastMedianTime => "Past Median Time",
            Metric::VirtualParentHashes => "Virtual Parent Hashes",
            Metric::VirtualDaaScore => "Virtual DAA Score",
        }
    }
}

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsData {
    pub unixtime: f64,

    // ---
    pub resident_set_size_bytes: u64,
    pub virtual_memory_size_bytes: u64,
    pub cpu_cores: u64,
    pub cpu_usage: f64,
    // ---
    pub fd_num: u64,
    pub disk_io_read_bytes: u64,
    pub disk_io_write_bytes: u64,
    pub disk_io_read_per_sec: f64,
    pub disk_io_write_per_sec: f64,
    // ---
    pub peers_connected: usize,
    // ---
    pub blocks_submitted: u64,
    pub header_counts: u64,
    pub dep_counts: u64,
    pub body_counts: u64,
    pub txs_counts: u64,
    pub chain_block_counts: u64,
    pub mass_counts: u64,
    // ---
    pub block_count: u64,
    pub tip_hashes: usize,
    pub difficulty: f64,
    pub past_median_time: u64,
    pub virtual_parent_hashes: usize,
    pub virtual_daa_score: u64,
}

impl MetricsData {
    pub fn new(unixtime: f64) -> Self {
        Self { unixtime, ..Default::default() }
    }
}

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub data: MetricsData,

    pub unixtime: f64,
    pub duration: f64,
    // ---
    pub resident_set_size_bytes: f64,
    pub virtual_memory_size_bytes: f64,
    pub cpu_cores: f64,
    pub cpu_usage: f64,
    // ---
    pub fd_num: f64,
    pub disk_io_read_bytes: f64,
    pub disk_io_write_bytes: f64,
    pub disk_io_read_per_sec: f64,
    pub disk_io_write_per_sec: f64,
    // ---
    pub peers_connected: f64,
    // ---
    pub blocks_submitted: f64,
    pub header_counts: f64,
    pub dep_counts: f64,
    pub body_counts: f64,
    pub txs_counts: f64,
    pub tps: f64,
    pub chain_block_counts: f64,
    pub mass_counts: f64,
    // ---
    pub block_count: f64,
    pub tip_hashes: f64,
    pub difficulty: f64,
    pub past_median_time: f64,
    pub virtual_parent_hashes: f64,
    pub virtual_daa_score: f64,
}

impl MetricsSnapshot {
    pub fn get(&self, metric: &Metric) -> f64 {
        match metric {
            // CpuCores
            Metric::CpuUsage => self.cpu_usage, // / self.cpu_cores,
            Metric::ResidentSetSizeBytes => self.resident_set_size_bytes,
            Metric::VirtualMemorySizeBytes => self.virtual_memory_size_bytes,
            Metric::FdNum => self.fd_num,
            Metric::DiskIoReadBytes => self.disk_io_read_bytes,
            Metric::DiskIoWriteBytes => self.disk_io_write_bytes,
            Metric::DiskIoReadPerSec => self.disk_io_read_per_sec,
            Metric::DiskIoWritePerSec => self.disk_io_write_per_sec,
            Metric::PeersConnected => self.peers_connected,

            // ---
            Metric::BlocksSubmitted => self.blocks_submitted,
            Metric::HeaderCount => self.header_counts,
            Metric::DepCounts => self.dep_counts,
            Metric::BodyCounts => self.body_counts,
            Metric::TxnCounts => self.txs_counts,
            Metric::Tps => self.tps,
            Metric::ChainBlockCounts => self.chain_block_counts,
            Metric::MassCounts => self.mass_counts,
            Metric::BlockCount => self.block_count,
            Metric::TipHashes => self.tip_hashes,
            Metric::Difficulty => self.difficulty,
            Metric::PastMedianTime => self.past_median_time,
            Metric::VirtualParentHashes => self.virtual_parent_hashes,
            Metric::VirtualDaaScore => self.virtual_daa_score,
        }
    }

    pub fn format(&self, metric: &Metric, si: bool, short: bool) -> String {
        format!("{}: {}", metric.title(), metric.format(self.get(metric), si, short))
    }
}

impl From<(&MetricsData, &MetricsData)> for MetricsSnapshot {
    fn from((a, b): (&MetricsData, &MetricsData)) -> Self {
        let duration = b.unixtime - a.unixtime;
        let tps = (b.txs_counts - a.txs_counts) as f64 * 1000. / duration;
        Self {
            unixtime: b.unixtime,
            duration,
            // ---
            cpu_usage: b.cpu_usage / b.cpu_cores as f64 * 100.0,
            cpu_cores: b.cpu_cores as f64,
            resident_set_size_bytes: b.resident_set_size_bytes as f64,
            virtual_memory_size_bytes: b.virtual_memory_size_bytes as f64,
            fd_num: b.fd_num as f64,
            disk_io_read_bytes: b.disk_io_read_bytes as f64,
            disk_io_write_bytes: b.disk_io_write_bytes as f64,
            disk_io_read_per_sec: b.disk_io_read_per_sec,
            disk_io_write_per_sec: b.disk_io_write_per_sec,
            // ---
            peers_connected: b.peers_connected as f64,
            blocks_submitted: b.blocks_submitted as f64,
            header_counts: b.header_counts as f64,
            dep_counts: b.dep_counts as f64,
            body_counts: b.body_counts as f64,
            txs_counts: b.txs_counts as f64,
            tps,
            chain_block_counts: b.chain_block_counts as f64,
            mass_counts: b.mass_counts as f64,
            // ---
            block_count: b.block_count as f64,
            tip_hashes: b.tip_hashes as f64,
            difficulty: b.difficulty,
            past_median_time: b.past_median_time as f64,
            virtual_parent_hashes: b.virtual_parent_hashes as f64,
            virtual_daa_score: b.virtual_daa_score as f64,

            data: b.clone(),
        }
    }
}

/// Display KB or KiB if `short` is false, otherwise if `short` is true
/// and the value is greater than 1MB or 1MiB, display units using [`as_data_size()`].
fn as_kb(bytes: f64, si: bool, short: bool) -> String {
    let unit = if si { 1000_f64 } else { 1024_f64 };
    if short && bytes > unit.powi(2) {
        as_data_size(bytes, si)
    } else {
        let suffix = if si { " KB" } else { " KiB" };
        let kb = bytes / unit; //(( * 100.) as u64) as f64 / 100.;
        format_with_precision(kb) + suffix
    }
}

/// Display MB or MiB if `short` is false, otherwise if `short` is true
/// and the value is greater than 1GB or 1GiB, display units using [`as_data_size()`].
fn as_mb(bytes: f64, si: bool, short: bool) -> String {
    let unit = if si { 1000_f64 } else { 1024_f64 };
    if short && bytes > unit.powi(3) {
        as_data_size(bytes, si)
    } else {
        let suffix = if si { " MB" } else { " MiB" };
        let mb = bytes / unit.powi(2); //(( * 100.) as u64) as f64 / 100.;
        format_with_precision(mb) + suffix
    }
}

/// Display GB or GiB if `short` is false, otherwise if `short` is true
/// and the value is greater than 1TB or 1TiB, display units using [`as_data_size()`].
fn _as_gb(bytes: f64, si: bool, short: bool) -> String {
    let unit = if si { 1000_f64 } else { 1024_f64 };
    if short && bytes > unit.powi(4) {
        as_data_size(bytes, si)
    } else {
        let suffix = if si { " GB" } else { " GiB" };
        let gb = bytes / unit.powi(3); //(( * 100.) as u64) as f64 / 100.;
        format_with_precision(gb) + suffix
    }
}

/// Display units dynamically formatted based on the size of the value.
fn as_data_size(bytes: f64, si: bool) -> String {
    let unit = if si { 1000_f64 } else { 1024_f64 };
    let mut size = bytes;
    let mut unit_str = "B";

    if size >= unit.powi(4) {
        size /= unit.powi(4);
        unit_str = " TB";
    } else if size >= unit.powi(3) {
        size /= unit.powi(3);
        unit_str = " GB";
    } else if size >= unit.powi(2) {
        size /= unit.powi(2);
        unit_str = " MB";
    } else if size >= unit {
        size /= unit;
        unit_str = " KB";
    }

    format_with_precision(size) + unit_str
}

/// Format supplied value as a float with 2 decimal places.
fn format_as_float(f: f64, short: bool) -> String {
    if short {
        if f < 1000.0 {
            format_with_precision(f)
        } else if f < 1000000.0 {
            format_with_precision(f / 1000.0) + " K"
        } else if f < 1000000000.0 {
            format_with_precision(f / 1000000.0) + " M"
        } else if f < 1000000000000.0 {
            format_with_precision(f / 1000000000.0) + " G"
        } else if f < 1000000000000000.0 {
            format_with_precision(f / 1000000000000.0) + " T"
        } else if f < 1000000000000000000.0 {
            format_with_precision(f / 1000000000000000.0) + " P"
        } else {
            format_with_precision(f / 1000000000000000000.0) + " E"
        }
    } else {
        f.separated_string()
    }
}

/// Format supplied value as a float with 2 decimal places.
fn format_with_precision(f: f64) -> String {
    if f.fract() < 0.01 {
        separated_float!(format!("{}", f.trunc()))
    } else {
        separated_float!(format!("{:.2}", f))
    }
}
