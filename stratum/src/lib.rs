//! Kaspa Stratum Mining Protocol Server
//!
//! This module provides a native Stratum protocol implementation for the Kaspa node,
//! allowing miners to connect directly to the node without requiring a separate pool server.
//!
//! The Stratum server is optional and can be enabled via feature flag or configuration.

pub mod error;
pub mod protocol;
pub mod server;
pub mod client;

pub use server::{StratumServer, StratumConfig, BlockSubmission};
pub use server::MiningJob;
pub use error::StratumError;

