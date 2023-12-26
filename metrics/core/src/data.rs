use borsh::{BorshDeserialize, BorshSerialize};
use separator::{separated_float, separated_int, separated_uint_with_output, Separatable};
use serde::{Deserialize, Serialize};
use workflow_core::enums::Describe;

#[derive(Describe, Debug, Clone, Copy, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricGroup {
    System,
    Storage,
    Bandwidth,
    Connections,
    Network,
}

impl std::fmt::Display for MetricGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetricGroup::System => write!(f, "system"),
            MetricGroup::Storage => write!(f, "storage"),
            MetricGroup::Bandwidth => write!(f, "bandwidth"),
            MetricGroup::Connections => write!(f, "connections"),
            MetricGroup::Network => write!(f, "network"),
        }
    }
}

impl MetricGroup {
    pub fn title(&self) -> &str {
        match self {
            MetricGroup::System => "System",
            MetricGroup::Storage => "Storage",
            MetricGroup::Bandwidth => "Bandwidth",
            MetricGroup::Connections => "Connections",
            MetricGroup::Network => "Network",
        }
    }
}

impl MetricGroup {
    pub fn iter() -> impl Iterator<Item = MetricGroup> {
        [MetricGroup::System, MetricGroup::Storage, MetricGroup::Connections, MetricGroup::Network].into_iter()
    }

    pub fn metrics(&self) -> impl Iterator<Item = &Metric> {
        match self {
            MetricGroup::System => [
                Metric::NodeCpuUsage,
                Metric::NodeResidentSetSizeBytes,
                Metric::NodeVirtualMemorySizeBytes,
                Metric::NodeFileHandlesCount,
            ]
            .as_slice()
            .iter(),
            MetricGroup::Storage => [
                Metric::NodeDiskIoReadBytes,
                Metric::NodeDiskIoReadPerSec,
                Metric::NodeDiskIoWriteBytes,
                Metric::NodeDiskIoWritePerSec,
            ]
            .as_slice()
            .iter(),
            MetricGroup::Bandwidth => [
                Metric::NodeTotalBytesTx,
                Metric::NodeTotalBytesTxPerSecond,
                Metric::NodeTotalBytesRx,
                Metric::NodeTotalBytesRxPerSecond,
                Metric::NodeBorshBytesTx,
                Metric::NodeBorshBytesTxPerSecond,
                Metric::NodeBorshBytesRx,
                Metric::NodeBorshBytesRxPerSecond,
                Metric::NodeP2pBytesTx,
                Metric::NodeP2pBytesTxPerSecond,
                Metric::NodeP2pBytesRx,
                Metric::NodeP2pBytesRxPerSecond,
                Metric::NodeGrpcUserBytesTx,
                Metric::NodeGrpcUserBytesTxPerSecond,
                Metric::NodeGrpcUserBytesRx,
                Metric::NodeGrpcUserBytesRxPerSecond,
                Metric::NodeJsonBytesTx,
                Metric::NodeJsonBytesTxPerSecond,
                Metric::NodeJsonBytesRx,
                Metric::NodeJsonBytesRxPerSecond,
            ]
            .as_slice()
            .iter(),
            MetricGroup::Connections => [
                Metric::NodeActivePeers,
                Metric::NodeBorshLiveConnections,
                Metric::NodeBorshConnectionAttempts,
                Metric::NodeBorshHandshakeFailures,
                Metric::NodeJsonLiveConnections,
                Metric::NodeJsonConnectionAttempts,
                Metric::NodeJsonHandshakeFailures,
            ]
            .as_slice()
            .iter(),
            MetricGroup::Network => [
                Metric::NodeBlocksSubmittedCount,
                Metric::NodeHeadersProcessedCount,
                Metric::NodeDependenciesProcessedCount,
                Metric::NodeBodiesProcessedCount,
                Metric::NodeTransactionsProcessedCount,
                Metric::NodeChainBlocksProcessedCount,
                Metric::NodeMassProcessedCount,
                Metric::NodeDatabaseBlocksCount,
                Metric::NodeDatabaseHeadersCount,
                Metric::NetworkMempoolSize,
                Metric::NetworkTransactionsPerSecond,
                Metric::NetworkTipHashesCount,
                Metric::NetworkDifficulty,
                Metric::NetworkPastMedianTime,
                Metric::NetworkVirtualParentHashesCount,
                Metric::NetworkVirtualDaaScore,
            ]
            .as_slice()
            .iter(),
        }
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
            | Metric::NodeActivePeers => MetricGroup::Connections,
            // --
            Metric::NodeBorshBytesRx
            | Metric::NodeBorshBytesTx
            | Metric::NodeJsonBytesTx
            | Metric::NodeJsonBytesRx
            | Metric::NodeP2pBytesTx
            | Metric::NodeP2pBytesRx
            | Metric::NodeGrpcUserBytesTx
            | Metric::NodeGrpcUserBytesRx
            | Metric::NodeTotalBytesRx
            | Metric::NodeTotalBytesTx
            | Metric::NodeBorshBytesRxPerSecond
            | Metric::NodeBorshBytesTxPerSecond
            | Metric::NodeJsonBytesTxPerSecond
            | Metric::NodeJsonBytesRxPerSecond
            | Metric::NodeP2pBytesTxPerSecond
            | Metric::NodeP2pBytesRxPerSecond
            | Metric::NodeGrpcUserBytesTxPerSecond
            | Metric::NodeGrpcUserBytesRxPerSecond
            | Metric::NodeTotalBytesRxPerSecond
            | Metric::NodeTotalBytesTxPerSecond => MetricGroup::Bandwidth,
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
            | Metric::NetworkMempoolSize
            | Metric::NetworkTransactionsPerSecond
            | Metric::NetworkTipHashesCount
            | Metric::NetworkDifficulty
            | Metric::NetworkPastMedianTime
            | Metric::NetworkVirtualParentHashesCount
            | Metric::NetworkVirtualDaaScore => MetricGroup::Network,
        }
    }
}

#[derive(Describe, Debug, Clone, Copy, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
    NodeActivePeers,
    NodeBorshLiveConnections,
    NodeBorshConnectionAttempts,
    NodeBorshHandshakeFailures,
    NodeJsonLiveConnections,
    NodeJsonConnectionAttempts,
    NodeJsonHandshakeFailures,
    // ---
    NodeTotalBytesTx,
    NodeTotalBytesRx,
    NodeTotalBytesTxPerSecond,
    NodeTotalBytesRxPerSecond,

    NodeP2pBytesTx,
    NodeP2pBytesRx,
    NodeP2pBytesTxPerSecond,
    NodeP2pBytesRxPerSecond,

    NodeBorshBytesTx,
    NodeBorshBytesRx,
    NodeBorshBytesTxPerSecond,
    NodeBorshBytesRxPerSecond,

    NodeGrpcUserBytesTx,
    NodeGrpcUserBytesRx,
    NodeGrpcUserBytesTxPerSecond,
    NodeGrpcUserBytesRxPerSecond,

    NodeJsonBytesTx,
    NodeJsonBytesRx,
    NodeJsonBytesTxPerSecond,
    NodeJsonBytesRxPerSecond,

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
    NetworkMempoolSize,
    NetworkTransactionsPerSecond,
    NetworkTipHashesCount,
    NetworkDifficulty,
    NetworkPastMedianTime,
    NetworkVirtualParentHashesCount,
    NetworkVirtualDaaScore,
}

impl Metric {
    // TODO - this will be refactored at a later date
    // as this requires changes and testing in /kos
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
            | Metric::NodeBorshBytesTx
            | Metric::NodeBorshBytesRx
            | Metric::NodeJsonBytesTx
            | Metric::NodeJsonBytesRx
            | Metric::NodeP2pBytesTx
            | Metric::NodeP2pBytesRx
            | Metric::NodeGrpcUserBytesTx
            | Metric::NodeGrpcUserBytesRx
            | Metric::NodeTotalBytesTx
            | Metric::NodeTotalBytesRx
            | Metric::NodeBorshBytesTxPerSecond
            | Metric::NodeBorshBytesRxPerSecond
            | Metric::NodeJsonBytesTxPerSecond
            | Metric::NodeJsonBytesRxPerSecond
            | Metric::NodeP2pBytesTxPerSecond
            | Metric::NodeP2pBytesRxPerSecond
            | Metric::NodeGrpcUserBytesTxPerSecond
            | Metric::NodeGrpcUserBytesRxPerSecond
            | Metric::NodeTotalBytesTxPerSecond
            | Metric::NodeTotalBytesRxPerSecond
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
            | Metric::NetworkMempoolSize
            | Metric::NetworkTransactionsPerSecond
            | Metric::NetworkTipHashesCount
            | Metric::NetworkDifficulty
            | Metric::NetworkPastMedianTime
            | Metric::NetworkVirtualParentHashesCount
            | Metric::NetworkVirtualDaaScore => "kaspa",
        }
    }

    pub fn is_key_performance_metric(&self) -> bool {
        matches!(
            self,
            Metric::NodeCpuUsage
                | Metric::NodeResidentSetSizeBytes
                | Metric::NodeFileHandlesCount
                | Metric::NodeDiskIoReadBytes
                | Metric::NodeDiskIoWriteBytes
                | Metric::NodeDiskIoReadPerSec
                | Metric::NodeDiskIoWritePerSec
                | Metric::NodeBorshBytesTx
                | Metric::NodeBorshBytesRx
                | Metric::NodeP2pBytesTx
                | Metric::NodeP2pBytesRx
                | Metric::NodeGrpcUserBytesTx
                | Metric::NodeGrpcUserBytesRx
                | Metric::NodeTotalBytesTx
                | Metric::NodeTotalBytesRx
                | Metric::NodeBorshBytesTxPerSecond
                | Metric::NodeBorshBytesRxPerSecond
                | Metric::NodeP2pBytesTxPerSecond
                | Metric::NodeP2pBytesRxPerSecond
                | Metric::NodeGrpcUserBytesTxPerSecond
                | Metric::NodeGrpcUserBytesRxPerSecond
                | Metric::NodeTotalBytesTxPerSecond
                | Metric::NodeTotalBytesRxPerSecond
                | Metric::NodeActivePeers
                | Metric::NetworkMempoolSize
                | Metric::NetworkTipHashesCount
                | Metric::NetworkTransactionsPerSecond
                | Metric::NodeTransactionsProcessedCount
                | Metric::NodeDatabaseBlocksCount
                | Metric::NodeDatabaseHeadersCount
        )
    }

    pub fn format(&self, f: f64, si: bool, short: bool) -> String {
        match self {
            Metric::NodeCpuUsage => {
                if f.is_nan() {
                    "---".to_string()
                } else {
                    format!("{:1.2}%", f)
                }
            }
            Metric::NodeResidentSetSizeBytes => as_mb(f, si, short),
            Metric::NodeVirtualMemorySizeBytes => as_mb(f, si, short),
            Metric::NodeFileHandlesCount => f.separated_string(),
            // --
            Metric::NodeDiskIoReadBytes => as_mb(f, si, short),
            Metric::NodeDiskIoWriteBytes => as_mb(f, si, short),
            Metric::NodeDiskIoReadPerSec => format!("{}/s", as_data_size(f, si)),
            Metric::NodeDiskIoWritePerSec => format!("{}/s", as_data_size(f, si)),
            // --
            Metric::NodeBorshLiveConnections => f.trunc().separated_string(),
            Metric::NodeBorshConnectionAttempts => f.trunc().separated_string(),
            Metric::NodeBorshHandshakeFailures => f.trunc().separated_string(),
            Metric::NodeJsonLiveConnections => f.trunc().separated_string(),
            Metric::NodeJsonConnectionAttempts => f.trunc().separated_string(),
            Metric::NodeJsonHandshakeFailures => f.trunc().separated_string(),
            Metric::NodeActivePeers => f.trunc().separated_string(),
            // --
            Metric::NodeBorshBytesTx => as_data_size(f, si),
            Metric::NodeBorshBytesRx => as_data_size(f, si),
            Metric::NodeJsonBytesTx => as_data_size(f, si),
            Metric::NodeJsonBytesRx => as_data_size(f, si),
            Metric::NodeP2pBytesTx => as_data_size(f, si),
            Metric::NodeP2pBytesRx => as_data_size(f, si),
            Metric::NodeGrpcUserBytesTx => as_data_size(f, si),
            Metric::NodeGrpcUserBytesRx => as_data_size(f, si),
            Metric::NodeTotalBytesTx => as_data_size(f, si),
            Metric::NodeTotalBytesRx => as_data_size(f, si),
            // --
            Metric::NodeBorshBytesTxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeBorshBytesRxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeJsonBytesTxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeJsonBytesRxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeP2pBytesTxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeP2pBytesRxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeGrpcUserBytesTxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeGrpcUserBytesRxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeTotalBytesTxPerSecond => format!("{}/s", as_kb(f, si, short)),
            Metric::NodeTotalBytesRxPerSecond => format!("{}/s", as_kb(f, si, short)),
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
            Metric::NetworkMempoolSize => format_as_float(f.trunc(), short),
            Metric::NetworkTransactionsPerSecond => format_as_float(f.trunc(), short),
            Metric::NetworkTipHashesCount => format_as_float(f, short),
            Metric::NetworkDifficulty => format_as_float(f, short),
            Metric::NetworkPastMedianTime => format_as_float(f, false),
            Metric::NetworkVirtualParentHashesCount => format_as_float(f, short),
            Metric::NetworkVirtualDaaScore => format_as_float(f, false),
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
            Metric::NodeDiskIoReadPerSec => ("Storage Read/s", "Stor Read"),
            Metric::NodeDiskIoWritePerSec => ("Storage Write/s", "Stor Write"),
            // --
            Metric::NodeActivePeers => ("Active p2p Peers", "Peers"),
            Metric::NodeBorshLiveConnections => ("Borsh Active Connections", "Borsh Conn"),
            Metric::NodeBorshConnectionAttempts => ("Borsh Connection Attempts", "Borsh Conn Att"),
            Metric::NodeBorshHandshakeFailures => ("Borsh Handshake Failures", "Borsh Failures"),
            Metric::NodeJsonLiveConnections => ("Json Active Connections", "Json Conn"),
            Metric::NodeJsonConnectionAttempts => ("Json Connection Attempts", "Json Conn Att"),
            Metric::NodeJsonHandshakeFailures => ("Json Handshake Failures", "Json Failures"),
            // --
            Metric::NodeBorshBytesTx => ("wRPC Borsh Tx", "Borsh Tx"),
            Metric::NodeBorshBytesRx => ("wRPC Borsh Rx", "Borsh Rx"),
            Metric::NodeJsonBytesTx => ("wRPC JSON Tx", "Json Tx"),
            Metric::NodeJsonBytesRx => ("wRPC JSON Rx", "Json Rx"),
            Metric::NodeP2pBytesTx => ("p2p Tx", "p2p Tx"),
            Metric::NodeP2pBytesRx => ("p2p Rx", "p2p Rx"),
            Metric::NodeGrpcUserBytesTx => ("gRPC Tx", "gRPC Tx"),
            Metric::NodeGrpcUserBytesRx => ("gRPC Rx", "gRPC Rx"),
            Metric::NodeTotalBytesTx => ("Total Tx", "Total Tx"),
            Metric::NodeTotalBytesRx => ("Total Rx", "Total Rx"),
            // --
            Metric::NodeBorshBytesTxPerSecond => ("wRPC Borsh Tx/s", "Borsh Tx/s"),
            Metric::NodeBorshBytesRxPerSecond => ("wRPC Borsh Rx/s", "Borsh Rx/s"),
            Metric::NodeJsonBytesTxPerSecond => ("wRPC JSON Tx/s", "JSON Tx/s"),
            Metric::NodeJsonBytesRxPerSecond => ("wRPC JSON Rx/s", "JSON Rx/s"),
            Metric::NodeP2pBytesTxPerSecond => ("p2p Tx/s", "p2p Tx/s"),
            Metric::NodeP2pBytesRxPerSecond => ("p2p Rx/s", "p2p Rx/s"),
            Metric::NodeGrpcUserBytesTxPerSecond => ("gRPC Tx/s", "gRPC Tx/s"),
            Metric::NodeGrpcUserBytesRxPerSecond => ("gRPC Rx/s", "gRPC Rx/s"),
            Metric::NodeTotalBytesTxPerSecond => ("Total Tx/s", "Total Tx/s"),
            Metric::NodeTotalBytesRxPerSecond => ("Total Rx/s", "Total Rx/s"),
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
            Metric::NetworkMempoolSize => ("Mempool Size", "Mempool"),
            Metric::NetworkTransactionsPerSecond => ("TPS", "TPS"),
            Metric::NetworkTipHashesCount => ("Tip Hashes", "Tip Hashes"),
            Metric::NetworkDifficulty => ("Network Difficulty", "Difficulty"),
            Metric::NetworkPastMedianTime => ("Past Median Time", "MT"),
            Metric::NetworkVirtualParentHashesCount => ("Virtual Parent Hashes", "Virt Parents"),
            Metric::NetworkVirtualDaaScore => ("Virtual DAA Score", "DAA"),
        }
    }
}

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsData {
    pub unixtime_millis: f64,

    // ---
    pub node_resident_set_size_bytes: u64,
    pub node_virtual_memory_size_bytes: u64,
    pub node_cpu_cores: u32,
    pub node_cpu_usage: f32,
    pub node_file_handles: u32,
    // ---
    pub node_disk_io_read_bytes: u64,
    pub node_disk_io_write_bytes: u64,
    pub node_disk_io_read_per_sec: f32,
    pub node_disk_io_write_per_sec: f32,
    // ---
    pub node_borsh_live_connections: u32,
    pub node_borsh_connection_attempts: u64,
    pub node_borsh_handshake_failures: u64,
    pub node_json_live_connections: u32,
    pub node_json_connection_attempts: u64,
    pub node_json_handshake_failures: u64,
    pub node_active_peers: u32,
    // ---
    pub node_borsh_bytes_tx: u64,
    pub node_borsh_bytes_rx: u64,
    pub node_json_bytes_tx: u64,
    pub node_json_bytes_rx: u64,
    pub node_p2p_bytes_tx: u64,
    pub node_p2p_bytes_rx: u64,
    pub node_grpc_user_bytes_tx: u64,
    pub node_grpc_user_bytes_rx: u64,
    pub node_total_bytes_tx: u64,
    pub node_total_bytes_rx: u64,

    pub node_borsh_bytes_tx_per_second: u64,
    pub node_borsh_bytes_rx_per_second: u64,
    pub node_json_bytes_tx_per_second: u64,
    pub node_json_bytes_rx_per_second: u64,
    pub node_p2p_bytes_tx_per_second: u64,
    pub node_p2p_bytes_rx_per_second: u64,
    pub node_grpc_user_bytes_tx_per_second: u64,
    pub node_grpc_user_bytes_rx_per_second: u64,
    pub node_total_bytes_tx_per_second: u64,
    pub node_total_bytes_rx_per_second: u64,
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
    pub network_mempool_size: u64,
    pub network_tip_hashes_count: u32,
    pub network_difficulty: f64,
    pub network_past_median_time: u64,
    pub network_virtual_parent_hashes_count: u32,
    pub network_virtual_daa_score: u64,
}

impl MetricsData {
    pub fn new(unixtime: f64) -> Self {
        Self { unixtime_millis: unixtime, ..Default::default() }
    }
}

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub data: MetricsData,

    pub unixtime_millis: f64,
    pub duration_millis: f64,
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
    pub node_borsh_bytes_tx: f64,
    pub node_borsh_bytes_rx: f64,
    pub node_json_bytes_tx: f64,
    pub node_json_bytes_rx: f64,
    pub node_p2p_bytes_tx: f64,
    pub node_p2p_bytes_rx: f64,
    pub node_grpc_user_bytes_tx: f64,
    pub node_grpc_user_bytes_rx: f64,
    pub node_total_bytes_tx: f64,
    pub node_total_bytes_rx: f64,

    pub node_borsh_bytes_tx_per_second: f64,
    pub node_borsh_bytes_rx_per_second: f64,
    pub node_json_bytes_tx_per_second: f64,
    pub node_json_bytes_rx_per_second: f64,
    pub node_p2p_bytes_tx_per_second: f64,
    pub node_p2p_bytes_rx_per_second: f64,
    pub node_grpc_user_bytes_tx_per_second: f64,
    pub node_grpc_user_bytes_rx_per_second: f64,
    pub node_total_bytes_tx_per_second: f64,
    pub node_total_bytes_rx_per_second: f64,

    // ---
    pub node_blocks_submitted_count: f64,
    pub node_headers_processed_count: f64,
    pub node_dependencies_processed_count: f64,
    pub node_bodies_processed_count: f64,
    pub node_transactions_processed_count: f64,
    pub node_chain_blocks_processed_count: f64,
    pub node_mass_processed_count: f64,
    // ---
    pub network_mempool_size: f64,
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
            Metric::NodeBorshBytesTx => self.node_borsh_bytes_tx,
            Metric::NodeBorshBytesRx => self.node_borsh_bytes_rx,
            Metric::NodeJsonBytesTx => self.node_json_bytes_tx,
            Metric::NodeJsonBytesRx => self.node_json_bytes_rx,
            Metric::NodeP2pBytesTx => self.node_p2p_bytes_tx,
            Metric::NodeP2pBytesRx => self.node_p2p_bytes_rx,
            Metric::NodeGrpcUserBytesTx => self.node_grpc_user_bytes_tx,
            Metric::NodeGrpcUserBytesRx => self.node_grpc_user_bytes_rx,
            Metric::NodeTotalBytesTx => self.node_total_bytes_tx,
            Metric::NodeTotalBytesRx => self.node_total_bytes_rx,

            Metric::NodeBorshBytesTxPerSecond => self.node_borsh_bytes_tx_per_second,
            Metric::NodeBorshBytesRxPerSecond => self.node_borsh_bytes_rx_per_second,
            Metric::NodeJsonBytesTxPerSecond => self.node_json_bytes_tx_per_second,
            Metric::NodeJsonBytesRxPerSecond => self.node_json_bytes_rx_per_second,
            Metric::NodeP2pBytesTxPerSecond => self.node_p2p_bytes_tx_per_second,
            Metric::NodeP2pBytesRxPerSecond => self.node_p2p_bytes_rx_per_second,
            Metric::NodeGrpcUserBytesTxPerSecond => self.node_grpc_user_bytes_tx_per_second,
            Metric::NodeGrpcUserBytesRxPerSecond => self.node_grpc_user_bytes_rx_per_second,
            Metric::NodeTotalBytesTxPerSecond => self.node_total_bytes_tx_per_second,
            Metric::NodeTotalBytesRxPerSecond => self.node_total_bytes_rx_per_second,
            // ---
            Metric::NodeBlocksSubmittedCount => self.node_blocks_submitted_count,
            Metric::NodeHeadersProcessedCount => self.node_headers_processed_count,
            Metric::NodeDependenciesProcessedCount => self.node_dependencies_processed_count,
            Metric::NodeBodiesProcessedCount => self.node_bodies_processed_count,
            Metric::NodeTransactionsProcessedCount => self.node_transactions_processed_count,
            Metric::NodeChainBlocksProcessedCount => self.node_chain_blocks_processed_count,
            Metric::NodeMassProcessedCount => self.node_mass_processed_count,
            // --
            Metric::NodeDatabaseBlocksCount => self.node_database_blocks_count,
            Metric::NodeDatabaseHeadersCount => self.node_database_headers_count,
            // --
            Metric::NetworkMempoolSize => self.network_mempool_size,
            Metric::NetworkTransactionsPerSecond => self.network_transactions_per_second,
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

#[inline(always)]
fn per_sec(a: u64, b: u64, duration_millis: f64) -> f64 {
    b.checked_sub(a).unwrap_or_default() as f64 * 1000. / duration_millis
}

impl From<(&MetricsData, &MetricsData)> for MetricsSnapshot {
    fn from((a, b): (&MetricsData, &MetricsData)) -> Self {
        let duration_millis = b.unixtime_millis - a.unixtime_millis;

        let network_transactions_per_second =
            per_sec(a.node_transactions_processed_count, b.node_transactions_processed_count, duration_millis);
        let node_borsh_bytes_tx_per_second = per_sec(a.node_borsh_bytes_tx, b.node_borsh_bytes_tx, duration_millis);
        let node_borsh_bytes_rx_per_second = per_sec(a.node_borsh_bytes_rx, b.node_borsh_bytes_rx, duration_millis);
        let node_json_bytes_tx_per_second = per_sec(a.node_json_bytes_tx, b.node_json_bytes_tx, duration_millis);
        let node_json_bytes_rx_per_second = per_sec(a.node_json_bytes_rx, b.node_json_bytes_rx, duration_millis);
        let node_p2p_bytes_tx_per_second = per_sec(a.node_p2p_bytes_tx, b.node_p2p_bytes_tx, duration_millis);
        let node_p2p_bytes_rx_per_second = per_sec(a.node_p2p_bytes_rx, b.node_p2p_bytes_rx, duration_millis);
        let node_grpc_user_bytes_tx_per_second = per_sec(a.node_grpc_user_bytes_tx, b.node_grpc_user_bytes_tx, duration_millis);
        let node_grpc_user_bytes_rx_per_second = per_sec(a.node_grpc_user_bytes_rx, b.node_grpc_user_bytes_rx, duration_millis);
        let node_total_bytes_tx_per_second = per_sec(a.node_total_bytes_tx, b.node_total_bytes_tx, duration_millis);
        let node_total_bytes_rx_per_second = per_sec(a.node_total_bytes_rx, b.node_total_bytes_rx, duration_millis);

        Self {
            unixtime_millis: b.unixtime_millis,
            duration_millis,
            // ---
            node_cpu_usage: b.node_cpu_usage as f64 / b.node_cpu_cores as f64 * 100.0,
            node_cpu_cores: b.node_cpu_cores as f64,
            node_resident_set_size_bytes: b.node_resident_set_size_bytes as f64,
            node_virtual_memory_size_bytes: b.node_virtual_memory_size_bytes as f64,
            node_file_handles: b.node_file_handles as f64,
            node_disk_io_read_bytes: b.node_disk_io_read_bytes as f64,
            node_disk_io_write_bytes: b.node_disk_io_write_bytes as f64,
            node_disk_io_read_per_sec: b.node_disk_io_read_per_sec as f64,
            node_disk_io_write_per_sec: b.node_disk_io_write_per_sec as f64,
            // ---
            node_borsh_active_connections: b.node_borsh_live_connections as f64,
            node_borsh_connection_attempts: b.node_borsh_connection_attempts as f64,
            node_borsh_handshake_failures: b.node_borsh_handshake_failures as f64,
            node_json_active_connections: b.node_json_live_connections as f64,
            node_json_connection_attempts: b.node_json_connection_attempts as f64,
            node_json_handshake_failures: b.node_json_handshake_failures as f64,
            node_active_peers: b.node_active_peers as f64,
            // ---
            node_borsh_bytes_tx: b.node_borsh_bytes_tx as f64,
            node_borsh_bytes_rx: b.node_borsh_bytes_rx as f64,
            node_json_bytes_tx: b.node_json_bytes_tx as f64,
            node_json_bytes_rx: b.node_json_bytes_rx as f64,
            node_p2p_bytes_tx: b.node_p2p_bytes_tx as f64,
            node_p2p_bytes_rx: b.node_p2p_bytes_rx as f64,
            node_grpc_user_bytes_tx: b.node_grpc_user_bytes_tx as f64,
            node_grpc_user_bytes_rx: b.node_grpc_user_bytes_rx as f64,
            node_total_bytes_tx: b.node_total_bytes_tx as f64,
            node_total_bytes_rx: b.node_total_bytes_rx as f64,

            node_borsh_bytes_tx_per_second,
            node_borsh_bytes_rx_per_second,
            node_json_bytes_tx_per_second,
            node_json_bytes_rx_per_second,
            node_p2p_bytes_tx_per_second,
            node_p2p_bytes_rx_per_second,
            node_grpc_user_bytes_tx_per_second,
            node_grpc_user_bytes_rx_per_second,
            node_total_bytes_tx_per_second,
            node_total_bytes_rx_per_second,
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
            network_mempool_size: b.network_mempool_size as f64,
            network_transactions_per_second,
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
    let mut unit_str = " B";

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
