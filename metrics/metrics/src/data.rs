use borsh::{BorshDeserialize, BorshSerialize};
use separator::{separated_float, separated_int, separated_uint_with_output, Separatable};
use serde::{Deserialize, Serialize};
use workflow_core::enums::Describe;

#[derive(Describe, Debug, Clone, Copy, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricGroup {
    System,
    Storage,
    Network,
    BlockDAG,
}

impl std::fmt::Display for MetricGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricGroup::System => write!(f, "system"),
            MetricGroup::Storage => write!(f, "storage"),
            MetricGroup::Network => write!(f, "network"),
            MetricGroup::BlockDAG => write!(f, "block-dag"),
        }
    }
}

impl MetricGroup {
    pub fn title(&self) -> &str {
        match self {
            MetricGroup::System => "System",
            MetricGroup::Storage => "Storage",
            MetricGroup::Network => "Network",
            MetricGroup::BlockDAG => "BlockDAG",
        }
    }
}

impl MetricGroup {
    pub fn iter() -> impl Iterator<Item = MetricGroup> {
        [MetricGroup::System, MetricGroup::Storage, MetricGroup::Network, MetricGroup::BlockDAG].into_iter()
    }

    pub fn metrics(&self) -> impl Iterator<Item = Metric> {
        match self {
            MetricGroup::System => vec![Metric::NodeCpuUsage, Metric::NodeResidentSetSizeBytes, Metric::NodeVirtualMemorySizeBytes],
            MetricGroup::Storage => vec![
                Metric::NodeFileHandlesCount,
                Metric::NodeDiskIoReadBytes,
                Metric::NodeDiskIoWriteBytes,
                Metric::NodeDiskIoReadPerSec,
                Metric::NodeDiskIoWritePerSec,
            ],
            MetricGroup::Network => vec![Metric::NodeActivePeers],
            MetricGroup::BlockDAG => vec![
                Metric::NodeBlocksSubmittedCount,
                Metric::NodeHeadersProcessedCount,
                Metric::NodeDependenciesProcessedCount,
                Metric::NodeBodiesProcessedCount,
                Metric::NodeTransactionsProcessedCount,
                Metric::NodeChainBlocksProcessedCount,
                Metric::NodeMassProcessedCount,
                Metric::NodeDatabaseBlocksCount,
                Metric::NodeDatabaseHeadersCount,
                Metric::NetworkTransactionsPerSecond,
                Metric::NetworkTipHashesCount,
                Metric::NetworkDifficulty,
                Metric::NetworkPastMedianTime,
                Metric::NetworkVirtualParentHashesCount,
                Metric::NetworkVirtualDaaScore,
            ],
        }
        .into_iter()
    }
}

impl From<Metric> for MetricGroup {
    fn from(value: Metric) -> Self {
        match value {
            Metric::NodeCpuUsage | Metric::NodeResidentSetSizeBytes | Metric::NodeVirtualMemorySizeBytes => MetricGroup::System,
            // --
            Metric::NodeFileHandlesCount
            | Metric::NodeDiskIoReadBytes
            | Metric::NodeDiskIoWriteBytes
            | Metric::NodeDiskIoReadPerSec
            | Metric::NodeDiskIoWritePerSec => MetricGroup::Storage,
            // --
            Metric::NodeBorshLiveConnections
            | Metric::NodeBorshConnectionAttempts
            | Metric::NodeBorshHandshakeFailures
            | Metric::NodeJsonLiveConnections
            | Metric::NodeJsonConnectionAttempts
            | Metric::NodeJsonHandshakeFailures
            | Metric::NodeActivePeers => MetricGroup::Network,
            // --
            Metric::NodeBlocksSubmittedCount
            | Metric::NodeHeadersProcessedCount
            | Metric::NodeDependenciesProcessedCount
            | Metric::NodeBodiesProcessedCount
            | Metric::NodeTransactionsProcessedCount
            | Metric::NodeChainBlocksProcessedCount
            | Metric::NodeMassProcessedCount
            // --
            | Metric::NodeDatabaseBlocksCount
            | Metric::NodeDatabaseHeadersCount
            // --
            | Metric::NetworkTransactionsPerSecond
            | Metric::NetworkTipHashesCount
            | Metric::NetworkDifficulty
            | Metric::NetworkPastMedianTime
            | Metric::NetworkVirtualParentHashesCount
            | Metric::NetworkVirtualDaaScore => MetricGroup::BlockDAG,
        }
    }
}

#[derive(Describe, Debug, Clone, Copy, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum Metric {
    // NodeCpuCores is used to normalize NodeCpuUsage metric
    // NodeCpuCores
    NodeCpuUsage,
    NodeResidentSetSizeBytes,
    NodeVirtualMemorySizeBytes,
    // ---
    NodeFileHandlesCount,
    NodeDiskIoReadBytes,
    NodeDiskIoWriteBytes,
    NodeDiskIoReadPerSec,
    NodeDiskIoWritePerSec,
    // ---
    NodeBorshLiveConnections,
    NodeBorshConnectionAttempts,
    NodeBorshHandshakeFailures,
    NodeJsonLiveConnections,
    NodeJsonConnectionAttempts,
    NodeJsonHandshakeFailures,
    NodeActivePeers,
    // ---
    NodeBlocksSubmittedCount,
    NodeHeadersProcessedCount,
    NodeDependenciesProcessedCount,
    NodeBodiesProcessedCount,
    NodeTransactionsProcessedCount,
    NodeChainBlocksProcessedCount,
    NodeMassProcessedCount,
    // --
    NodeDatabaseBlocksCount,
    NodeDatabaseHeadersCount,
    // --
    NetworkTransactionsPerSecond,
    NetworkTipHashesCount,
    NetworkDifficulty,
    NetworkPastMedianTime,
    NetworkVirtualParentHashesCount,
    NetworkVirtualDaaScore,
}

impl Metric {
    pub fn group(&self) -> &'static str {
        match self {
            Metric::NodeCpuUsage
            | Metric::NodeResidentSetSizeBytes
            | Metric::NodeVirtualMemorySizeBytes
            | Metric::NodeFileHandlesCount
            | Metric::NodeDiskIoReadBytes
            | Metric::NodeDiskIoWriteBytes
            | Metric::NodeDiskIoReadPerSec
            | Metric::NodeDiskIoWritePerSec
            | Metric::NodeBorshLiveConnections
            | Metric::NodeBorshConnectionAttempts
            | Metric::NodeBorshHandshakeFailures
            | Metric::NodeJsonLiveConnections
            | Metric::NodeJsonConnectionAttempts
            | Metric::NodeJsonHandshakeFailures
            | Metric::NodeActivePeers => "system",
            // --
            Metric::NodeBlocksSubmittedCount
            | Metric::NodeHeadersProcessedCount
            | Metric::NodeDependenciesProcessedCount
            | Metric::NodeBodiesProcessedCount
            | Metric::NodeTransactionsProcessedCount
            | Metric::NodeChainBlocksProcessedCount
            | Metric::NodeMassProcessedCount
            | Metric::NodeDatabaseBlocksCount
            | Metric::NodeDatabaseHeadersCount
            | Metric::NetworkTransactionsPerSecond
            | Metric::NetworkTipHashesCount
            | Metric::NetworkDifficulty
            | Metric::NetworkPastMedianTime
            | Metric::NetworkVirtualParentHashesCount
            | Metric::NetworkVirtualDaaScore => "kaspa",
        }
    }

    pub fn format(&self, f: f64, si: bool, short: bool) -> String {
        match self {
            Metric::NodeCpuUsage => format!("{:1.2}%", f),
            Metric::NodeResidentSetSizeBytes => as_mb(f, si, short),
            Metric::NodeVirtualMemorySizeBytes => as_mb(f, si, short),
            Metric::NodeFileHandlesCount => f.separated_string(),
            // --
            Metric::NodeDiskIoReadBytes => as_mb(f, si, short),
            Metric::NodeDiskIoWriteBytes => as_mb(f, si, short),
            Metric::NodeDiskIoReadPerSec => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeDiskIoWritePerSec => format!("{}/s", as_kb(f, si, short)),
            // --
            Metric::NodeBorshLiveConnections => f.separated_string(),
            Metric::NodeBorshConnectionAttempts => f.separated_string(),
            Metric::NodeBorshHandshakeFailures => f.separated_string(),
            Metric::NodeJsonLiveConnections => f.separated_string(),
            Metric::NodeJsonConnectionAttempts => f.separated_string(),
            Metric::NodeJsonHandshakeFailures => f.separated_string(),
            Metric::NodeActivePeers => f.separated_string(),
            // --
            Metric::NodeBlocksSubmittedCount => format_as_float(f, short),
            Metric::NodeHeadersProcessedCount => format_as_float(f, short),
            Metric::NodeDependenciesProcessedCount => format_as_float(f, short),
            Metric::NodeBodiesProcessedCount => format_as_float(f, short),
            Metric::NodeTransactionsProcessedCount => format_as_float(f, short),
            Metric::NodeChainBlocksProcessedCount => format_as_float(f, short),
            Metric::NodeMassProcessedCount => format_as_float(f, short),
            // --
            Metric::NodeDatabaseHeadersCount => format_as_float(f, short),
            Metric::NodeDatabaseBlocksCount => format_as_float(f, short),
            // --
            Metric::NetworkTransactionsPerSecond => format_as_float(f.trunc(), short),
            Metric::NetworkTipHashesCount => format_as_float(f, short),
            Metric::NetworkDifficulty => format_as_float(f, short),
            Metric::NetworkPastMedianTime => format_as_float(f, short),
            Metric::NetworkVirtualParentHashesCount => format_as_float(f, short),
            Metric::NetworkVirtualDaaScore => format_as_float(f, short),
        }
    }

    pub fn title(&self) -> (&str, &str) {
        match self {
            Metric::NodeCpuUsage => ("CPU", "CPU"),
            Metric::NodeResidentSetSizeBytes => ("Resident Memory", "Memory"),
            Metric::NodeVirtualMemorySizeBytes => ("Virtual Memory", "Virtual"),
            // --
            Metric::NodeFileHandlesCount => ("File Handles", "Handles"),
            Metric::NodeDiskIoReadBytes => ("Storage Read", "Stor Read"),
            Metric::NodeDiskIoWriteBytes => ("Storage Write", "Stor Write"),
            Metric::NodeDiskIoReadPerSec => ("Storage Read", "Store Read"),
            Metric::NodeDiskIoWritePerSec => ("Storage Write", "Stor Write"),
            // --
            Metric::NodeActivePeers => ("Active Peers", "Peers"),
            Metric::NodeBorshLiveConnections => ("Borsh Active Connections", "Borsh Conn"),
            Metric::NodeBorshConnectionAttempts => ("Borsh Connection Attempts", "Borsh Conn Att"),
            Metric::NodeBorshHandshakeFailures => ("Borsh Handshake Failures", "Borsh Failures"),
            Metric::NodeJsonLiveConnections => ("Json Active Connections", "Json Conn"),
            Metric::NodeJsonConnectionAttempts => ("Json Connection Attempts", "Json Conn Att"),
            Metric::NodeJsonHandshakeFailures => ("Json Handshake Failures", "Json Failures"),
            // --
            Metric::NodeBlocksSubmittedCount => ("Submitted Blocks", "Blocks"),
            Metric::NodeHeadersProcessedCount => ("Processed Headers", "Headers"),
            Metric::NodeDependenciesProcessedCount => ("Processed Dependencies", "Dependencies"),
            Metric::NodeBodiesProcessedCount => ("Processed Bodies", "Bodies"),
            Metric::NodeTransactionsProcessedCount => ("Processed Transactions", "Transactions"),
            Metric::NodeChainBlocksProcessedCount => ("Chain Blocks", "Chain Blocks"),
            Metric::NodeMassProcessedCount => ("Processed Mass Counts", "Mass Processed"),
            // --
            Metric::NodeDatabaseBlocksCount => ("Database Blocks", "DB Blocks"),
            Metric::NodeDatabaseHeadersCount => ("Database Headers", "DB Headers"),
            // --
            Metric::NetworkTransactionsPerSecond => ("TPS", "TPS"),
            Metric::NetworkTipHashesCount => ("Tip Hashes", "Tip Hashes"),
            Metric::NetworkDifficulty => ("Network Difficulty", "Difficulty"),
            Metric::NetworkPastMedianTime => ("Past Median Time", "Median T"),
            Metric::NetworkVirtualParentHashesCount => ("Virtual Parent Hashes", "Virt. Parents"),
            Metric::NetworkVirtualDaaScore => ("Virtual DAA Score", "DAA"),
        }
    }
}

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsData {
    pub unixtime: f64,

    // ---
    pub node_resident_set_size_bytes: u64,
    pub node_virtual_memory_size_bytes: u64,
    pub node_cpu_cores: u64,
    pub node_cpu_usage: f64,
    // ---
    pub node_file_handles: u64,
    pub node_disk_io_read_bytes: u64,
    pub node_disk_io_write_bytes: u64,
    pub node_disk_io_read_per_sec: f64,
    pub node_disk_io_write_per_sec: f64,
    // ---
    pub node_borsh_live_connections: u64,
    pub node_borsh_connection_attempts: u64,
    pub node_borsh_handshake_failures: u64,
    pub node_json_live_connections: u64,
    pub node_json_connection_attempts: u64,
    pub node_json_handshake_failures: u64,
    pub node_active_peers: u64,
    // ---
    pub node_blocks_submitted_count: u64,
    pub node_headers_processed_count: u64,
    pub node_dependencies_processed_count: u64,
    pub node_bodies_processed_count: u64,
    pub node_transactions_processed_count: u64,
    pub node_chain_blocks_processed_count: u64,
    pub node_mass_processed_count: u64,
    // ---
    pub node_database_blocks_count: u64,
    pub node_database_headers_count: u64,
    // --
    pub network_tip_hashes_count: u64,
    pub network_difficulty: f64,
    pub network_past_median_time: u64,
    pub network_virtual_parent_hashes_count: u64,
    pub network_virtual_daa_score: u64,
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
    pub node_resident_set_size_bytes: f64,
    pub node_virtual_memory_size_bytes: f64,
    pub node_cpu_cores: f64,
    pub node_cpu_usage: f64,
    // ---
    pub node_file_handles: f64,
    pub node_disk_io_read_bytes: f64,
    pub node_disk_io_write_bytes: f64,
    pub node_disk_io_read_per_sec: f64,
    pub node_disk_io_write_per_sec: f64,
    // ---
    pub node_borsh_active_connections: f64,
    pub node_borsh_connection_attempts: f64,
    pub node_borsh_handshake_failures: f64,
    pub node_json_active_connections: f64,
    pub node_json_connection_attempts: f64,
    pub node_json_handshake_failures: f64,
    pub node_active_peers: f64,
    // ---
    pub node_blocks_submitted_count: f64,
    pub node_headers_processed_count: f64,
    pub node_dependencies_processed_count: f64,
    pub node_bodies_processed_count: f64,
    pub node_transactions_processed_count: f64,
    pub node_chain_blocks_processed_count: f64,
    pub node_mass_processed_count: f64,
    // ---
    pub network_transactions_per_second: f64,
    pub node_database_blocks_count: f64,
    pub node_database_headers_count: f64,
    pub network_tip_hashes_count: f64,
    pub network_difficulty: f64,
    pub network_past_median_time: f64,
    pub network_virtual_parent_hashes_count: f64,
    pub network_virtual_daa_score: f64,
}

impl MetricsSnapshot {
    pub fn get(&self, metric: &Metric) -> f64 {
        match metric {
            // CpuCores
            Metric::NodeCpuUsage => self.node_cpu_usage, // / self.cpu_cores,
            Metric::NodeResidentSetSizeBytes => self.node_resident_set_size_bytes,
            Metric::NodeVirtualMemorySizeBytes => self.node_virtual_memory_size_bytes,
            Metric::NodeFileHandlesCount => self.node_file_handles,
            Metric::NodeDiskIoReadBytes => self.node_disk_io_read_bytes,
            Metric::NodeDiskIoWriteBytes => self.node_disk_io_write_bytes,
            Metric::NodeDiskIoReadPerSec => self.node_disk_io_read_per_sec,
            Metric::NodeDiskIoWritePerSec => self.node_disk_io_write_per_sec,
            // ---
            Metric::NodeActivePeers => self.node_active_peers,
            Metric::NodeBorshLiveConnections => self.node_borsh_active_connections,
            Metric::NodeBorshConnectionAttempts => self.node_borsh_connection_attempts,
            Metric::NodeBorshHandshakeFailures => self.node_borsh_handshake_failures,
            Metric::NodeJsonLiveConnections => self.node_json_active_connections,
            Metric::NodeJsonConnectionAttempts => self.node_json_connection_attempts,
            Metric::NodeJsonHandshakeFailures => self.node_json_handshake_failures,
            // ---
            Metric::NodeBlocksSubmittedCount => self.node_blocks_submitted_count,
            Metric::NodeHeadersProcessedCount => self.node_headers_processed_count,
            Metric::NodeDependenciesProcessedCount => self.node_dependencies_processed_count,
            Metric::NodeBodiesProcessedCount => self.node_bodies_processed_count,
            Metric::NodeTransactionsProcessedCount => self.node_transactions_processed_count,
            Metric::NetworkTransactionsPerSecond => self.network_transactions_per_second,
            Metric::NodeChainBlocksProcessedCount => self.node_chain_blocks_processed_count,
            Metric::NodeMassProcessedCount => self.node_mass_processed_count,
            Metric::NodeDatabaseBlocksCount => self.node_database_blocks_count,
            Metric::NodeDatabaseHeadersCount => self.node_database_headers_count,
            Metric::NetworkTipHashesCount => self.network_tip_hashes_count,
            Metric::NetworkDifficulty => self.network_difficulty,
            Metric::NetworkPastMedianTime => self.network_past_median_time,
            Metric::NetworkVirtualParentHashesCount => self.network_virtual_parent_hashes_count,
            Metric::NetworkVirtualDaaScore => self.network_virtual_daa_score,
        }
    }

    pub fn format(&self, metric: &Metric, si: bool, short: bool) -> String {
        if short {
            format!("{}: {}", metric.title().1, metric.format(self.get(metric), si, short))
        } else {
            format!("{}: {}", metric.title().0, metric.format(self.get(metric), si, short))
        }
    }
}

impl From<(&MetricsData, &MetricsData)> for MetricsSnapshot {
    fn from((a, b): (&MetricsData, &MetricsData)) -> Self {
        let duration = b.unixtime - a.unixtime;
        let tps = b.node_transactions_processed_count.checked_sub(a.node_transactions_processed_count).unwrap_or_default() as f64
            * 1000.
            / duration;
        Self {
            unixtime: b.unixtime,
            duration,
            // ---
            node_cpu_usage: b.node_cpu_usage / b.node_cpu_cores as f64 * 100.0,
            node_cpu_cores: b.node_cpu_cores as f64,
            node_resident_set_size_bytes: b.node_resident_set_size_bytes as f64,
            node_virtual_memory_size_bytes: b.node_virtual_memory_size_bytes as f64,
            node_file_handles: b.node_file_handles as f64,
            node_disk_io_read_bytes: b.node_disk_io_read_bytes as f64,
            node_disk_io_write_bytes: b.node_disk_io_write_bytes as f64,
            node_disk_io_read_per_sec: b.node_disk_io_read_per_sec,
            node_disk_io_write_per_sec: b.node_disk_io_write_per_sec,
            // ---
            node_borsh_active_connections: b.node_borsh_live_connections as f64,
            node_borsh_connection_attempts: b.node_borsh_connection_attempts as f64,
            node_borsh_handshake_failures: b.node_borsh_handshake_failures as f64,
            node_json_active_connections: b.node_json_live_connections as f64,
            node_json_connection_attempts: b.node_json_connection_attempts as f64,
            node_json_handshake_failures: b.node_json_handshake_failures as f64,
            node_active_peers: b.node_active_peers as f64,
            // ---
            node_blocks_submitted_count: b.node_blocks_submitted_count as f64,
            node_headers_processed_count: b.node_headers_processed_count as f64,
            node_dependencies_processed_count: b.node_dependencies_processed_count as f64,
            node_bodies_processed_count: b.node_bodies_processed_count as f64,
            node_transactions_processed_count: b.node_transactions_processed_count as f64,
            node_chain_blocks_processed_count: b.node_chain_blocks_processed_count as f64,
            node_mass_processed_count: b.node_mass_processed_count as f64,
            // ---
            node_database_blocks_count: b.node_database_blocks_count as f64,
            node_database_headers_count: b.node_database_headers_count as f64,
            // --
            network_transactions_per_second: tps,
            network_tip_hashes_count: b.network_tip_hashes_count as f64,
            network_difficulty: b.network_difficulty,
            network_past_median_time: b.network_past_median_time as f64,
            network_virtual_parent_hashes_count: b.network_virtual_parent_hashes_count as f64,
            network_virtual_daa_score: b.network_virtual_daa_score as f64,

            data: b.clone(),
        }
    }
}

/// Display KB or KiB if `short` is false, otherwise if `short` is true
/// and the value is greater than 1MB or 1MiB, display units using [`as_data_size()`].
pub fn as_kb(bytes: f64, si: bool, short: bool) -> String {
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
pub fn as_mb(bytes: f64, si: bool, short: bool) -> String {
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
pub fn as_gb(bytes: f64, si: bool, short: bool) -> String {
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
pub fn as_data_size(bytes: f64, si: bool) -> String {
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
