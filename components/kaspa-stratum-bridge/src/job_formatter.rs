//! Job Formatting Helper
//!
//! This module provides helper functions for formatting job parameters
//! for different ASIC miner types. All formatting logic preserves exact
//! existing behavior from the codebase.

use crate::hasher::{generate_iceriver_job_params, generate_job_header, generate_large_job_params};
use crate::miner_detection::is_iceriver;
use kaspa_hashes::Hash;
use serde_json::Value;

/// Format job parameters for a mining.notify message
///
/// This function preserves the EXACT formatting logic from the codebase:
/// - IceRiver: Single hex string (80 chars) using Hash::to_string()
/// - BzMiner: Single hex string (80 chars) using big-endian hash
/// - Legacy/Bitmain: Array of 4 u64 values + timestamp number
///
/// # Arguments
/// * `job_id` - The job ID as a u64
/// * `pre_pow_hash` - The pre-PoW hash for the block
/// * `timestamp` - The block timestamp
/// * `remote_app` - The remote app string (for miner type detection)
/// * `use_big_job` - Whether to use big job format (for BzMiner detection)
///
/// # Returns
/// A Vec<Value> containing the formatted job parameters:
/// - [0]: job_id as String
/// - [1]: Job data (format depends on miner type)
/// - [2]: Timestamp (only for Legacy/Bitmain format) []
pub fn format_job_params(job_id: u64, pre_pow_hash: &Hash, timestamp: u64, remote_app: &str, use_big_job: bool) -> Vec<Value> {
    let mut job_params = vec![Value::String(job_id.to_string())];

    // Detect miner type - preserve exact detection order from codebase
    let is_iceriver_flag = is_iceriver(remote_app);

    // Format based on miner type - preserve exact logic from codebase
    // Note: The order of checks matches the original code (BzMiner first, then IceRiver, then Legacy)
    // This is functionally equivalent since is_iceriver and use_big_job are mutually exclusive
    if use_big_job && !is_iceriver_flag {
        // BzMiner format - single hex string (big endian hash)
        // Convert Hash to bytes for BzMiner format
        let header_bytes = pre_pow_hash.as_bytes();
        let large_params = generate_large_job_params(&header_bytes, timestamp);
        job_params.push(Value::String(large_params));
    } else if is_iceriver_flag {
        // IceRiver format - single hex string (uses Hash::to_string() to match working stratum code)
        // This matches Ghostpool and other working implementations
        let iceriver_params = generate_iceriver_job_params(pre_pow_hash, timestamp);
        job_params.push(Value::String(iceriver_params));
    } else {
        // Legacy format - array + number (for Bitmain and other miners)
        let header_bytes = pre_pow_hash.as_bytes();
        let job_header = generate_job_header(&header_bytes);
        job_params.push(Value::Array(job_header.iter().map(|&v| Value::Number(v.into())).collect()));
        job_params.push(Value::Number(timestamp.into()));
    }

    job_params
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash;

    #[test]
    fn test_job_params_iceriver_format() {
        // Test IceRiver format returns 2 params: [job_id, hex_string]
        let hash = Hash::default();
        let params = format_job_params(123, &hash, 1000, "IceRiver KS2L", false);

        assert_eq!(params.len(), 2, "IceRiver format should have 2 params");
        assert!(params[0].is_string(), "First param should be job_id string");
        assert!(params[1].is_string(), "Second param should be hex string");

        // Verify hex string length (80 chars = 64 hash + 16 timestamp)
        if let Value::String(hex_str) = &params[1] {
            assert_eq!(hex_str.len(), 80, "IceRiver hex string should be 80 chars");
        }
    }

    #[test]
    fn test_job_params_bitmain_format() {
        // Test Bitmain/Legacy format returns 3 params: [job_id, array, timestamp]
        let hash = Hash::default();
        let params = format_job_params(456, &hash, 2000, "GodMiner", false);

        assert_eq!(params.len(), 3, "Bitmain format should have 3 params");
        assert!(params[0].is_string(), "First param should be job_id string");
        assert!(params[1].is_array(), "Second param should be array");
        assert!(params[2].is_number(), "Third param should be timestamp number");

        // Verify array has 4 elements (u64 values)
        if let Value::Array(arr) = &params[1] {
            assert_eq!(arr.len(), 4, "Legacy format array should have 4 elements");
        }
    }

    #[test]
    fn test_job_params_bzminer_format() {
        // Test BzMiner format returns 2 params: [job_id, hex_string]
        let hash = Hash::default();
        let params = format_job_params(789, &hash, 3000, "BzMiner", true);

        assert_eq!(params.len(), 2, "BzMiner format should have 2 params");
        assert!(params[0].is_string(), "First param should be job_id string");
        assert!(params[1].is_string(), "Second param should be hex string");

        // Verify hex string length (80 chars)
        if let Value::String(hex_str) = &params[1] {
            assert_eq!(hex_str.len(), 80, "BzMiner hex string should be 80 chars");
        }
    }

    #[test]
    fn test_job_params_legacy_format() {
        // Test Legacy format (non-Bitmain, non-IceRiver, non-BzMiner)
        let hash = Hash::default();
        let params = format_job_params(999, &hash, 4000, "Goldshell", false);

        assert_eq!(params.len(), 3, "Legacy format should have 3 params");
        assert!(params[0].is_string(), "First param should be job_id string");
        assert!(params[1].is_array(), "Second param should be array");
        assert!(params[2].is_number(), "Third param should be timestamp number");
    }
}
