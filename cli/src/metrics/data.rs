use borsh::{BorshDeserialize, BorshSerialize};
use separator::Separatable;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use workflow_core::{enums::Describe, sendable::Sendable};

#[derive(Describe, Debug, Clone, Eq, PartialEq, Hash, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum Metric {
    // CpuCores is used to normalize CpuUsage metric
    // CpuCores
    CpuUsage,
    ResidentSetSizeBytes,
    VirtualMemorySizeBytes,
    FdNum,
    DiskIoReadBytes,
    DiskIoWriteBytes,
    DiskIoReadPerSec,
    DiskIoWritePerSec,
    // ---
    BlocksSubmitted,
    HeaderCount,
    DepCounts,
    BodyCounts,
    TxnCounts,
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
            Metric::BlocksSubmitted
            | Metric::HeaderCount
            | Metric::DepCounts
            | Metric::BodyCounts
            | Metric::TxnCounts
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
}

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsData {
    pub unixtime: f64,

    // ---
    pub resident_set_size_bytes: u64,
    pub virtual_memory_size_bytes: u64,
    pub cpu_cores: u64,
    pub cpu_usage: f64,
    pub fd_num: u64,
    pub disk_io_read_bytes: u64,
    pub disk_io_write_bytes: u64,
    pub disk_io_read_per_sec: f64,
    pub disk_io_write_per_sec: f64,
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
    pub fd_num: f64,
    pub disk_io_read_bytes: f64,
    pub disk_io_write_bytes: f64,
    pub disk_io_read_per_sec: f64,
    pub disk_io_write_per_sec: f64,
    // ---
    pub blocks_submitted: f64,
    pub header_counts: f64,
    pub dep_counts: f64,
    pub body_counts: f64,
    pub txs_counts: f64,
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
    pub fn get(&self, metric: &Metric) -> Sendable<JsValue> {
        let v = match metric {
            // CpuCores
            Metric::CpuUsage => JsValue::from(self.cpu_usage / self.cpu_cores),
            Metric::ResidentSetSizeBytes => JsValue::from(self.resident_set_size_bytes),
            Metric::VirtualMemorySizeBytes => JsValue::from(self.virtual_memory_size_bytes),
            Metric::FdNum => JsValue::from(self.fd_num),
            Metric::DiskIoReadBytes => JsValue::from(self.disk_io_read_bytes),
            Metric::DiskIoWriteBytes => JsValue::from(self.disk_io_write_bytes),
            Metric::DiskIoReadPerSec => JsValue::from(self.disk_io_read_per_sec),
            Metric::DiskIoWritePerSec => JsValue::from(self.disk_io_write_per_sec),

            // ---
            Metric::BlocksSubmitted => JsValue::from(self.blocks_submitted),
            Metric::HeaderCount => JsValue::from(self.header_counts),
            Metric::DepCounts => JsValue::from(self.dep_counts),
            Metric::BodyCounts => JsValue::from(self.body_counts),
            Metric::TxnCounts => JsValue::from(self.txs_counts),
            Metric::ChainBlockCounts => JsValue::from(self.chain_block_counts),
            Metric::MassCounts => JsValue::from(self.mass_counts),
            Metric::BlockCount => JsValue::from(self.block_count),
            Metric::TipHashes => JsValue::from(self.tip_hashes),
            Metric::Difficulty => JsValue::from(self.difficulty),
            Metric::PastMedianTime => JsValue::from(self.past_median_time),
            Metric::VirtualParentHashes => JsValue::from(self.virtual_parent_hashes),
            Metric::VirtualDaaScore => JsValue::from(self.virtual_daa_score),
        };

        Sendable(v)
    }

    pub fn format(&self, metric: &Metric, si: bool) -> String {
        match metric {
            Metric::CpuUsage => format!("CPU: {:1.2}%", self.cpu_usage / self.cpu_cores * 100.0),
            Metric::ResidentSetSizeBytes => {
                format!("Resident Memory: {}", as_gb(self.resident_set_size_bytes, si))
            }
            Metric::VirtualMemorySizeBytes => {
                format!("Virtual Memory: {}", as_gb(self.virtual_memory_size_bytes, si))
            }
            Metric::FdNum => format!("File Handles: {}", self.fd_num.separated_string()),
            Metric::DiskIoReadBytes => format!("Storage Read: {}", as_gb(self.disk_io_read_bytes, si)),
            Metric::DiskIoWriteBytes => format!("Storage Write: {}", as_gb(self.disk_io_write_bytes, si)),
            Metric::DiskIoReadPerSec => format!("Storage Read: {}/s", as_kb(self.disk_io_read_per_sec, si)),
            Metric::DiskIoWritePerSec => format!("Storage Write: {}/s", as_kb(self.disk_io_write_per_sec, si)),
            // --
            Metric::BlocksSubmitted => format!("Blocks Submitted: {}", self.blocks_submitted.separated_string()),
            Metric::HeaderCount => format!("Headers: {}", self.header_counts.separated_string()),
            Metric::DepCounts => format!("Dependencies: {}", self.dep_counts.separated_string()),
            Metric::BodyCounts => format!("Body Counts: {}", self.body_counts.separated_string()),
            Metric::TxnCounts => format!("Transactions: {}", self.txs_counts.separated_string()),
            Metric::ChainBlockCounts => format!("Chain Blocks: {}", self.chain_block_counts.separated_string()),
            Metric::MassCounts => format!("Mass Counts: {}", self.mass_counts.separated_string()),
            Metric::BlockCount => format!("Blocks: {}", self.block_count.separated_string()),
            Metric::TipHashes => format!("Tip Hashes: {}", self.tip_hashes.separated_string()),
            Metric::Difficulty => {
                format!("Difficulty: {}", self.difficulty.separated_string())
            }
            Metric::PastMedianTime => format!("Past Median Time: {}", self.past_median_time.separated_string()),
            Metric::VirtualParentHashes => format!("Virtual Parent Hashes: {}", self.virtual_parent_hashes.separated_string()),
            Metric::VirtualDaaScore => format!("Virtual DAA Score: {}", self.virtual_daa_score.separated_string()),
        }
    }
}

impl From<(&MetricsData, &MetricsData)> for MetricsSnapshot {
    fn from((a, b): (&MetricsData, &MetricsData)) -> Self {
        Self {
            unixtime: b.unixtime,
            duration: b.unixtime - a.unixtime,
            // ---
            cpu_usage: b.cpu_usage as f64,
            cpu_cores: b.cpu_cores as f64,
            resident_set_size_bytes: b.resident_set_size_bytes as f64,
            virtual_memory_size_bytes: b.virtual_memory_size_bytes as f64,
            fd_num: b.fd_num as f64,
            disk_io_read_bytes: b.disk_io_read_bytes as f64,
            disk_io_write_bytes: b.disk_io_write_bytes as f64,
            disk_io_read_per_sec: b.disk_io_read_per_sec,
            disk_io_write_per_sec: b.disk_io_write_per_sec,
            // ---
            blocks_submitted: b.blocks_submitted as f64,
            header_counts: b.header_counts as f64,
            dep_counts: b.dep_counts as f64,
            body_counts: b.body_counts as f64,
            txs_counts: b.txs_counts as f64,
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

fn as_kb(bytes: f64, si: bool) -> String {
    let unit = if si { 1000. } else { 1024. };
    let suffix = if si { " KB" } else { " KiB" };
    let kb = ((bytes / unit * 100.) as u64) as f64 / 100.;
    (kb).separated_string() + suffix
}

// fn format_storage_gb(bytes : f64) -> String {
//     let gb = ((bytes / 1024. / 1024. / 1024. * 100.) as u64) as f64 / 100.;
//     (gb).separated_string()
// }

fn as_gb(bytes: f64, si: bool) -> String {
    let unit = if si { 1000. } else { 1024. };
    let suffix = if si { " GB" } else { " GiB" };
    let gb = ((bytes / unit / unit / unit * 100.) as u64) as f64 / 100.;
    (gb).separated_string() + suffix
}
