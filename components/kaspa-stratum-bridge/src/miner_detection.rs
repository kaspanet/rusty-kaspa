//! Miner Type Detection
//!
//! This module provides helper functions for detecting different ASIC miner types
//! based on the remote_app string. All detection logic preserves exact existing behavior.

use crate::constants::{BITMAIN_KEYWORDS, ICERIVER_KEYWORDS};

/// Miner type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinerType {
    /// IceRiver miners (IceRiverMiner, ICM, etc.)
    IceRiver,
    /// BzMiner (detected via use_big_job flag)
    BzMiner,
    /// Bitmain miners (GodMiner, Antminer, etc.)
    Bitmain,
    /// Legacy format miners (everything else)
    Legacy,
}

/// Detect miner type based on remote_app and use_big_job flag
///
/// This function preserves the EXACT detection logic from the codebase:
/// 1. First checks for IceRiver (iceriver, icemining, icm - case-insensitive)
/// 2. Then checks for Bitmain (godminer, bitmain, antminer - case-insensitive)
/// 3. Then checks use_big_job flag for BzMiner
/// 4. Otherwise returns Legacy
pub fn detect_miner_type(remote_app: &str, use_big_job: bool) -> MinerType {
    let remote_app_lower = remote_app.to_lowercase();
    
    // Check IceRiver first (case-insensitive)
    if ICERIVER_KEYWORDS.iter().any(|&keyword| remote_app_lower.contains(keyword)) {
        return MinerType::IceRiver;
    }
    
    // Check Bitmain (case-insensitive)
    if BITMAIN_KEYWORDS.iter().any(|&keyword| remote_app_lower.contains(keyword)) {
        return MinerType::Bitmain;
    }
    
    // Check BzMiner (requires use_big_job flag)
    if use_big_job {
        return MinerType::BzMiner;
    }
    
    // Default to Legacy
    MinerType::Legacy
}

/// Check if miner is Bitmain (preserves exact existing logic)
///
/// This function uses case-insensitive matching for:
/// - "godminer"
/// - "bitmain"
/// - "antminer"
pub fn is_bitmain(remote_app: &str) -> bool {
    let remote_app_lower = remote_app.to_lowercase();
    BITMAIN_KEYWORDS.iter().any(|&keyword| remote_app_lower.contains(keyword))
}

/// Check if miner is IceRiver (preserves exact existing logic)
///
/// NOTE: There are TWO different IceRiver detection patterns in the codebase:
/// 1. In client_handler.rs: Uses lowercase matching (iceriver, icemining, icm)
/// 2. In default_client.rs: Uses case-sensitive "IceRiver" check
///
/// This function uses the lowercase pattern (most common).
/// For the case-sensitive variant, use `is_iceriver_case_sensitive()`.
pub fn is_iceriver(remote_app: &str) -> bool {
    let remote_app_lower = remote_app.to_lowercase();
    ICERIVER_KEYWORDS.iter().any(|&keyword| remote_app_lower.contains(keyword))
}

/// Check if miner is IceRiver using case-sensitive "IceRiver" check
///
/// This preserves the exact behavior from default_client.rs line ~376
/// which uses: `remote_app.contains("IceRiver")` (case-sensitive)
pub fn is_iceriver_case_sensitive(remote_app: &str) -> bool {
    remote_app.contains("IceRiver")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmain_detection() {
        assert!(is_bitmain("GodMiner v1.0"));
        assert!(is_bitmain("BITMAIN-ASIC"));
        assert!(is_bitmain("antminer-ks"));
        assert!(is_bitmain("SomePrefixBitmainSuffix"));
        assert!(!is_bitmain("IceRiver KS2L"));
        assert!(!is_bitmain("BzMiner"));
    }

    #[test]
    fn test_iceriver_detection() {
        assert!(is_iceriver("IceRiver KS2L"));
        assert!(is_iceriver("ICERIVER"));
        assert!(is_iceriver("icemining"));
        assert!(is_iceriver("ICM-123"));
        assert!(!is_iceriver("BzMiner"));
        assert!(!is_iceriver("GodMiner"));
    }

    #[test]
    fn test_iceriver_case_sensitive() {
        assert!(is_iceriver_case_sensitive("IceRiver KS2L"));
        assert!(!is_iceriver_case_sensitive("iceriver")); // Case-sensitive!
        assert!(!is_iceriver_case_sensitive("ICERIVER"));
    }

    #[test]
    fn test_detect_miner_type() {
        assert_eq!(detect_miner_type("IceRiver KS2L", false), MinerType::IceRiver);
        assert_eq!(detect_miner_type("GodMiner", false), MinerType::Bitmain);
        assert_eq!(detect_miner_type("BzMiner", true), MinerType::BzMiner);
        assert_eq!(detect_miner_type("BzMiner", false), MinerType::Legacy);
        assert_eq!(detect_miner_type("Goldshell", false), MinerType::Legacy);
    }
}
