pub mod body_processor;
pub mod deps_manager;
pub mod header_processor;
pub mod monitor;
pub mod pruning_processor;
pub mod tx_receipts_manager;
pub mod receipts_errors;
pub mod virtual_processor;

/// Re-export from consensus core
pub use kaspa_consensus_core::api::counters::{ProcessingCounters, ProcessingCountersSnapshot};
