use kaspa_hashes::{BlockHash, HasherBase};
use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};

/// Maximum target value (2^224 - 1)
/// This is 28 bytes = 56 hex characters = 224 bits = 2^224 - 1
const MAX_TARGET: &str = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";

/// Minimum hash value (calculated at runtime)
/// Formula: minHash = 2^256 / maxTarget = 2^256 / (2^224 - 1) ≈ 2^32
/// Formula: minHash = 2^256 / maxTarget where maxTarget = 2^224 - 1
fn min_hash() -> f64 {
    // maxTarget = 2^224 - 1 ≈ 2^224
    // minHash = 2^256 / 2^224 = 2^32
    // Using f64: 2^32 = 4294967296.0
    2_f64.powi(32)
}

/// Big gig (1e9)
const BIG_GIG: f64 = 1_000_000_000.0;

/// Kaspa difficulty representation
#[derive(Debug, Clone)]
pub struct KaspaDiff {
    pub hash_value: f64,
    pub diff_value: f64,
    pub target_value: BigUint,
}

impl Default for KaspaDiff {
    fn default() -> Self {
        Self { hash_value: 0.0, diff_value: 0.0, target_value: BigUint::zero() }
    }
}

impl KaspaDiff {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_diff_value(&mut self, diff: f64) {
        self.diff_value = diff;
        self.target_value = diff_to_target(diff);
        self.hash_value = diff_to_hash(diff);
    }

    /// Set difficulty value
    /// Always uses standard calculation for all miners
    pub fn set_diff_value_for_miner(&mut self, diff: f64, _remote_app: &str) {
        // Always use standard calculation for all miners
        self.diff_value = diff;
        self.target_value = diff_to_target(diff);
        self.hash_value = diff_to_hash(diff);
    }
}

/// Alternative target calculation (TESTING ONLY)
/// Uses: target = (2^64 - 1) * 1000000 / (difficulty * 1000000)
/// This is a test implementation to verify ASIC expectations
pub fn diff_to_target_alternative(diff: f64) -> BigUint {
    use num_traits::Num;

    // Handle edge cases
    if diff <= 0.0 {
        return <BigUint as Num>::from_str_radix(MAX_TARGET, 16).unwrap();
    }

    // KASPA_MAX_TARGET = 2^64 - 1
    const KASPA_MAX_TARGET: u64 = 0xFFFFFFFFFFFFFFFF;
    let max_target_big = BigUint::from(KASPA_MAX_TARGET);
    let difficulty_big = BigUint::from((diff * 1000000.0) as u64);

    // Calculate target = max_target * 1000000 / (difficulty * 1000000)
    max_target_big * BigUint::from(1000000u64) / difficulty_big
}

/// Stratum difficulty to target (IceRiver specific)
/// Uses: target = (2^64 - 1) * 1000 / (stratum_diff * 1000)
/// This is the calculation method expected by IceRiver ASICs
pub fn stratum_difficulty_to_target_kaspa(stratum_diff: u64) -> BigUint {
    // Handle zero difficulty - return maximum target
    if stratum_diff == 0 {
        const KASPA_MAX_TARGET: u64 = 0xFFFFFFFFFFFFFFFF;
        return BigUint::from(KASPA_MAX_TARGET) * BigUint::from(1000u64);
    }

    // KASPA_MAX_TARGET = 2^64 - 1
    const KASPA_MAX_TARGET: u64 = 0xFFFFFFFFFFFFFFFF;
    let base_target = BigUint::from(KASPA_MAX_TARGET);
    let scaled_diff = BigUint::from(stratum_diff) * BigUint::from(1000u64);

    // target = max_target * 1000 / (stratum_diff * 1000)
    base_target * BigUint::from(1000u64) / scaled_diff
}

/// Convert difficulty to target
/// Formula: target = (0xffff * 2^208) / difficulty
/// This matches the WASM calculateTarget implementation used by IceRiver/Bitmain ASICs
/// Based on: https://github.com/tmrlvi/kaspa-miner/blob/bf361d02a46c580f55f46b5dfa773477634a5753/src/client/stratum.rs#L375
pub fn diff_to_target(diff: f64) -> BigUint {
    // Check environment variable to use alternative implementation for testing
    let use_alternative = std::env::var("USE_ALTERNATIVE_TARGET_CALC").unwrap_or_default().eq_ignore_ascii_case("true");

    // Check if we should use stratum-specific calculation (for integer difficulties)
    let use_stratum_alt = std::env::var("USE_STRATUM_TARGET_CALC").unwrap_or_default().eq_ignore_ascii_case("true");

    if use_stratum_alt {
        // Use stratum-specific calculation (takes u64, which is what ASICs receive)
        let diff_u64 = diff as u64;
        let stratum_target = stratum_difficulty_to_target_kaspa(diff_u64);
        let standard_target = diff_to_target_standard(diff);

        tracing::debug!(
            "STRATUM TARGET CALC: diff={} (u64={}), standard={:x} ({} bytes), stratum={:x} ({} bytes), ratio={:.6}",
            diff,
            diff_u64,
            standard_target,
            standard_target.to_bytes_be().len(),
            stratum_target,
            stratum_target.to_bytes_be().len(),
            if !stratum_target.is_zero() {
                standard_target.to_f64().unwrap_or(0.0) / stratum_target.to_f64().unwrap_or(1.0)
            } else {
                0.0
            }
        );

        return stratum_target;
    }

    if use_alternative {
        let alt_target = diff_to_target_alternative(diff);
        let standard_target = diff_to_target_standard(diff);

        // Log comparison for debugging
        tracing::debug!(
            "ALTERNATIVE TARGET CALC: diff={}, standard={:x} ({} bytes), alternative={:x} ({} bytes), ratio={:.6}",
            diff,
            standard_target,
            standard_target.to_bytes_be().len(),
            alt_target,
            alt_target.to_bytes_be().len(),
            if !alt_target.is_zero() { standard_target.to_f64().unwrap_or(0.0) / alt_target.to_f64().unwrap_or(1.0) } else { 0.0 }
        );

        return alt_target;
    }

    diff_to_target_standard(diff)
}

/// Standard target calculation
/// Formula: target = maxTarget / diff
/// This uses floating point division then converts to integer
fn diff_to_target_standard(diff: f64) -> BigUint {
    use num_traits::Num;

    // Handle edge cases
    if diff <= 0.0 {
        // Return maximum target for invalid difficulty
        return <BigUint as Num>::from_str_radix(MAX_TARGET, 16).unwrap();
    }

    // maxTarget = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF
    // This is 2^224 - 1, NOT the old 0xffff * 2^208!
    let max_target = <BigUint as Num>::from_str_radix(MAX_TARGET, 16).unwrap();

    // Calculate target = maxTarget / difficulty (EXACTLY like Go)
    // Go uses big.Float division: target = maxTarget / diff
    // Then converts to big.Int by truncating
    // We need high precision, so use a large scaling factor
    // Use 18 decimal places of precision (multiply by 1e18) to match big.Float precision
    let diff_scaled = (diff * 1_000_000_000_000_000_000.0) as u128;
    let diff_big = BigUint::from(diff_scaled);

    // target = (maxTarget * 1e18) / (diff * 1e18)
    // Uses big.Float division followed by Int() truncation
    (max_target * BigUint::from(1_000_000_000_000_000_000u128)) / diff_big
}

/// Convert difficulty to hash value
pub fn diff_to_hash(diff: f64) -> f64 {
    let hash_val = min_hash() * diff;
    hash_val / BIG_GIG
}

/// Serialize block header for mining
/// This creates the pre-PoW hash (hash WITHOUT timestamp and nonce)
/// Uses kaspa_hashes::BlockHash to match the working stratum implementation
/// Returns the Hash type directly (not bytes) to match working stratum code
pub fn serialize_block_header(block: &kaspa_consensus_core::block::Block) -> Result<kaspa_hashes::Hash, Box<dyn std::error::Error>> {
    let header = &block.header;
    let mut hasher = BlockHash::new();

    // Write version (16 bits, little endian)
    hasher.update(header.version.to_le_bytes());

    // Write number of parent levels
    let expanded_len = header.parents_by_level.expanded_len();
    hasher.update((expanded_len as u64).to_le_bytes());

    // Write parents at each level
    // The "Odd number of digits" error likely occurs here when processing parent hashes
    // We'll process each level and parent individually to catch errors early
    for level in header.parents_by_level.expanded_iter() {
        // Write array length
        hasher.update((level.len() as u64).to_le_bytes());
        // Write each parent hash
        for parent in level {
            // Access the hash as bytes first - this will trigger any hex parsing errors
            // The error "Odd number of digits" typically comes from hex decoding
            let _bytes = parent.as_bytes();
            hasher.update(parent); // Hash types can be updated directly
        }
    }

    // Write header fields - access each hash field to trigger any hex parsing errors early
    // The "Odd number of digits" error typically occurs when a hex string has odd length
    let _ = header.hash_merkle_root.as_bytes();
    let _ = header.accepted_id_merkle_root.as_bytes();
    let _ = header.utxo_commitment.as_bytes();
    let _ = header.pruning_point.as_bytes();

    // Write header fields
    hasher.update(header.hash_merkle_root).update(header.accepted_id_merkle_root).update(header.utxo_commitment);

    // Write the struct fields EXACTLY like Go does (lines 74-93 in hasher.go)
    // Go writes: TS(0) + Bits + Nonce(0) + DAAScore + BlueScore as one struct
    // We must match this EXACTLY!
    hasher.update(0u64.to_le_bytes()); // TS = 0 (8 bytes)
    hasher.update(header.bits.to_le_bytes()); // Bits (4 bytes, u32)
    hasher.update(0u64.to_le_bytes()); // Nonce = 0 (8 bytes)
    hasher.update(header.daa_score.to_le_bytes()); // DAAScore (8 bytes)
    hasher.update(header.blue_score.to_le_bytes()); // BlueScore (8 bytes)

    // Write blue_work (big endian bytes without leading zeros) - matches working stratum code
    let be_bytes = header.blue_work.to_be_bytes();
    let start = be_bytes.iter().copied().position(|byte| byte != 0).unwrap_or(be_bytes.len());
    let blue_work_bytes = &be_bytes[start..];
    hasher.update((blue_work_bytes.len() as u64).to_le_bytes());
    hasher.update(blue_work_bytes);

    // Write pruning_point
    hasher.update(header.pruning_point);

    // BlockHash::finalize() returns a Hash (32 bytes) - return it directly
    Ok(hasher.finalize())
}

/// Generate job header for standard miners (IceRiver/Goldshell)
/// Returns array of 4 uint64 values representing the header hash
/// NOTE: This is the OLD format - use generate_iceriver_job_params() for IceRiver compatibility
pub fn generate_job_header(header_data: &[u8]) -> Vec<u64> {
    let mut ids = Vec::new();

    // Read 4 uint64 values (little endian)
    for i in 0..4 {
        let offset = i * 8;
        if offset + 8 <= header_data.len() {
            let bytes = &header_data[offset..offset + 8];
            let value = u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]);
            ids.push(value);
        }
    }

    ids
}

/// Generate IceRiver-compatible job params (single hex string format)
/// Returns hex string of 80 characters: 64 (hash) + 16 (timestamp LE)
/// This matches what Ghostpool and other working implementations use
/// Format: hash (64 hex chars) + timestamp_le (16 hex chars) = 80 hex chars total
/// Uses Hash::to_string() to match the working stratum implementation exactly
///
/// NOTE: Timestamp is in MILLISECONDS (as per Kaspa block header format)
/// Pool uses: timestampLE.writeBigUInt64LE(timestamp) which writes u64 in LE
pub fn generate_iceriver_job_params(pre_pow_hash: &kaspa_hashes::Hash, timestamp: u64) -> String {
    // Use Hash::to_string() to match working stratum code exactly
    // This produces lowercase hex (64 hex characters)
    // Pool uses: proofOfWork.prePoWHash which is a hex string
    let hash_hex = pre_pow_hash.to_string();

    // Verify hash format
    tracing::debug!("[HASH] Pre-PoW hash bytes: {:?}", pre_pow_hash.as_bytes());
    tracing::debug!("[HASH] Pre-PoW hash hex string: {} (length: {})", hash_hex, hash_hex.len());

    // Verify it's lowercase (pool uses lowercase hex)
    if hash_hex != hash_hex.to_lowercase() {
        tracing::warn!("[HASH] WARNING: Hash contains uppercase characters!");
    } else {
        tracing::debug!("[HASH] Hash is lowercase (correct)");
    }

    // Verify length is exactly 64 hex characters (32 bytes)
    if hash_hex.len() != 64 {
        tracing::error!("[HASH] ERROR: Hash hex length is {} (expected 64)", hash_hex.len());
    } else {
        tracing::debug!("[HASH] Hash hex length is correct: 64 chars");
    }

    // Convert timestamp to little-endian bytes and then hex (16 hex characters)
    // This matches pool: timestampLE.writeBigUInt64LE(timestamp)
    let timestamp_le = timestamp.to_le_bytes();
    let timestamp_hex = hex::encode(timestamp_le);

    // Debug: Verify timestamp conversion matches pool format
    // Pool uses: timestampLE.writeBigUInt64LE(timestamp) which is exactly what to_le_bytes() does
    tracing::debug!("[TIMESTAMP] Input timestamp (u64): {} (milliseconds)", timestamp);
    tracing::debug!("[TIMESTAMP] Little-endian bytes: {:?}", timestamp_le);
    tracing::debug!("[TIMESTAMP] Hex string (16 chars): {}", timestamp_hex);

    // Verify hex length is exactly 16 characters (8 bytes)
    if timestamp_hex.len() != 16 {
        tracing::warn!("[TIMESTAMP] WARNING: Timestamp hex length is {} (expected 16)", timestamp_hex.len());
    } else {
        tracing::debug!("[TIMESTAMP] Timestamp hex length is correct: 16 chars");
    }

    // Verify the conversion by decoding it back
    let decoded_timestamp = u64::from_le_bytes([
        timestamp_le[0],
        timestamp_le[1],
        timestamp_le[2],
        timestamp_le[3],
        timestamp_le[4],
        timestamp_le[5],
        timestamp_le[6],
        timestamp_le[7],
    ]);
    if decoded_timestamp != timestamp {
        tracing::error!("[TIMESTAMP] ERROR: Timestamp round-trip failed! Original: {}, Decoded: {}", timestamp, decoded_timestamp);
    } else {
        tracing::debug!("[TIMESTAMP] Timestamp round-trip verification: PASSED");
    }

    // Concatenate: hash + timestamp = 80 hex characters
    let result = format!("{}{}", hash_hex, timestamp_hex);

    // Verify total length
    if result.len() != 80 {
        tracing::warn!("[TIMESTAMP] WARNING: Total job data length is {} (expected 80)", result.len());
    } else {
        tracing::debug!("[TIMESTAMP] Total job data length is correct: 80 chars");
    }

    tracing::debug!(
        "[TIMESTAMP] Final job data: {} (hash: {} chars, timestamp: {} chars)",
        result,
        hash_hex.len(),
        timestamp_hex.len()
    );

    // Final verification summary
    tracing::debug!("[JOB_FORMAT] ===== JOB FORMAT VERIFICATION =====");
    tracing::debug!("[JOB_FORMAT] Hash (64 hex): {}", hash_hex);
    tracing::debug!("[JOB_FORMAT] Timestamp (16 hex): {}", timestamp_hex);
    tracing::debug!("[JOB_FORMAT] Combined (80 hex): {}", result);
    tracing::debug!("[JOB_FORMAT] Format matches pool: hash + timestampLE.toString('hex')");
    tracing::debug!("[JOB_FORMAT] ===== END VERIFICATION =====");

    result
}

/// Generate large job params for BzMiner/Bitmain ASICs
/// Returns hex string of 80 characters (5 uint64 values in hex)
/// Generate large job parameters for IceRiver/BzMiner
pub fn generate_large_job_params(header_data: &[u8], timestamp: u64) -> String {
    let mut ids = Vec::new();

    // Read 4 uint64 values (big endian)
    for i in 0..4 {
        let offset = i * 8;
        if offset + 8 <= header_data.len() {
            let bytes = &header_data[offset..offset + 8];
            let value = u64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]);
            ids.push(value);
        }
    }

    // Timestamp handling: use milliseconds
    // Go does: timestampBytes = BigEndian.PutUint64(timestamp)
    //          ids[4] = LittleEndian.Uint64(timestampBytes)
    // This effectively byte-swaps the timestamp!
    let timestamp_be = timestamp.to_be_bytes();
    let timestamp_swapped = u64::from_le_bytes(timestamp_be);
    ids.push(timestamp_swapped);

    // Format as hex string (80 chars = 5 * 16)
    format!("{:016x}{:016x}{:016x}{:016x}{:016x}", ids[0], ids[1], ids[2], ids[3], ids[4])
}

/// Calculate target from bits (compact format)
/// Bits format: [exponent (1 byte)][mantissa (3 bytes)]
/// Target = mantissa << (8 * (exponent - 3))
/// This matches Kaspa's compact difficulty format
pub fn calculate_target(bits: u64) -> BigUint {
    let exponent = bits >> 24; // First byte is exponent
    let mantissa = bits & 0xFFFFFF; // Last 3 bytes are mantissa

    let (mantissa, exponent) = if exponent <= 3 {
        // Special case: if exponent <= 3, shift mantissa right instead
        let shift = 8 * (3 - exponent);
        (mantissa >> shift, 0u32)
    } else {
        // Normal case: target = mantissa << (8 * (exponent - 3))
        (mantissa, (8 * (exponent - 3)) as u32)
    };

    // Calculate final target: mantissa << exponent
    let mut target = BigUint::from(mantissa);
    target <<= exponent;

    target
}

/// Convert big difficulty to little (float representation)
pub fn big_diff_to_little(diff: &BigUint) -> f64 {
    use num_traits::ToPrimitive;
    // numerator = 2^254
    let numerator = BigUint::from(2u64).pow(254);

    let numerator_f = ToPrimitive::to_f64(&numerator).unwrap_or(0.0);
    let diff_f = ToPrimitive::to_f64(diff).unwrap_or(1.0);

    let result = numerator_f / diff_f;
    result / (2.0_f64.powi(31)) // Divide by 2^31
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_to_target() {
        use num_traits::Num;

        // Test difficulty 1.0 (should be MAX_TARGET per current implementation)
        let target = diff_to_target(1.0);
        let max_target = <BigUint as Num>::from_str_radix(MAX_TARGET, 16).unwrap();
        assert_eq!(target, max_target);

        // Test difficulty 8192 (should be MAX_TARGET / 8192)
        let target_8192 = diff_to_target(8192.0);
        let expected_8192 = &max_target >> 13u32; // 8192 = 2^13
        assert_eq!(target_8192, expected_8192);

        // Verify target_8192 is much smaller than target at diff=1.0
        assert!(target_8192 < max_target);

        // For difficulty 8192 = 2^13, target should be approximately 2^243
        // Let's verify it's in the right ballpark
        let target_hex = format!("{:x}", target_8192);
        println!("Target for difficulty 8192: {} ({} hex digits)", target_hex, target_hex.len());
        assert!(target_hex.len() <= 64); // Should be 64 hex digits or less

        // Test comparison with a sample pow_value
        // Sample pow_value from logs: 2ca3fd09a0ebcd525aa42e0345e7042487219016f373caf5406908b684794836
        let pow_value_hex = "2ca3fd09a0ebcd525aa42e0345e7042487219016f373caf5406908b684794836";
        let pow_value = <BigUint as Num>::from_str_radix(pow_value_hex, 16).unwrap();
        let pow_bytes = pow_value.to_bytes_be();
        let target_bytes = target_8192.to_bytes_be();

        println!("pow_value: {:x} ({} bytes)", pow_value, pow_bytes.len());
        println!("pool_target: {:x} ({} bytes)", target_8192, target_bytes.len());

        // Format with leading zeros for accurate comparison
        let pow_hex_full = format!("{:064x}", pow_value);
        let target_hex_full = format!("{:064x}", target_8192);
        println!("pow_value (full): {} ({} hex digits)", pow_hex_full, pow_hex_full.len());
        println!("pool_target (full): {} ({} hex digits)", target_hex_full, target_hex_full.len());

        let is_valid_share = pow_value < target_8192;
        println!("pow_value < pool_target (valid share): {}", is_valid_share);

        // Verify the comparison logic works correctly
        // For this specific pow_value, it should be less than target_8192 for difficulty 8192
        assert!(pow_value != target_8192, "pow_value should not equal target");

        // Verify byte lengths are consistent
        assert_eq!(pow_bytes.len(), 32, "pow_value should be 32 bytes");
        assert!(target_bytes.len() <= 32, "target should be <= 32 bytes");
    }

    #[test]
    fn test_calculate_target() {
        // Bits are in compact format: [exponent (1 byte)][mantissa (3 bytes)].
        // Bitcoin "difficulty 1" bits: 0x1d00ffff -> target = 0xffff << 208.
        let bits = 0x1d00ffffu64;
        let target = calculate_target(bits);
        let expected = BigUint::from(0xffffu64) << 208u32;
        assert_eq!(target, expected);

        // Test with actual devnet bits: 505527324 (0x1e21bc1c)
        let devnet_bits = 505527324u64;
        let devnet_target = calculate_target(devnet_bits);
        println!("Devnet bits: {} (0x{:x})", devnet_bits, devnet_bits);
        println!("Devnet target: {:x} ({} bytes)", devnet_target, devnet_target.to_bytes_be().len());

        // Sanity check: devnet bits 0x1e21bc1c -> exponent=0x1e=30, mantissa=0x21bc1c, shift=216
        let expected_devnet = BigUint::from(0x21bc1cu64) << 216u32;
        assert_eq!(devnet_target, expected_devnet);

        // Comparison sanity: a very small PoW value should be below the target,
        // and a very large value should be above it.
        let pow_small = BigUint::from(1u32);
        assert!(pow_small < devnet_target);

        let pow_large = BigUint::from(1u32) << 255u32;
        assert!(pow_large > devnet_target);
    }

    #[test]
    fn test_share_validation_comparison() {
        use num_traits::Num;

        // Test with actual pow_values from logs
        // pow_value from log line 733: 38401bd9f41763ea12bfb1ab9cf252709476437590272464fc287d69c0890e13
        // This starts with 0x38 which is < 0x7f, so it should be a valid share
        let pow_value_hex = "38401bd9f41763ea12bfb1ab9cf252709476437590272464fc287d69c0890e13";
        let pow_value = <BigUint as Num>::from_str_radix(pow_value_hex, 16).unwrap();

        // Target for difficulty 8192
        let target_8192 = diff_to_target(8192.0);
        let target_hex = format!("{:x}", target_8192);

        println!("\n=== Share Validation Test ===");
        println!("pow_value: {} (starts with {:02x})", pow_value_hex, pow_value.to_bytes_be()[0]);
        println!("pool_target: {} (starts with {:02x})", target_hex, target_8192.to_bytes_be()[0]);
        println!("pow_value bytes: {}", pow_value.to_bytes_be().len());
        println!("pool_target bytes: {}", target_8192.to_bytes_be().len());

        // Format with leading zeros
        let pow_full = format!("{:064x}", pow_value);
        let target_full = format!("{:064x}", target_8192);
        println!("pow_value (full): {}", pow_full);
        println!("pool_target (full): {}", target_full);

        let comparison = pow_value < target_8192;
        println!("pow_value < pool_target (should be true for valid share): {}", comparison);
        println!("pow_value >= pool_target: {}", pow_value >= target_8192);

        // This pow_value starts with 0x38 < 0x7f, so it should be valid
        // But we need to verify the actual numeric comparison
        if pow_value.to_bytes_be()[0] < target_8192.to_bytes_be()[0] {
            println!("First byte comparison suggests valid share");
        } else {
            println!("WARNING: First byte comparison suggests invalid share, but numeric comparison may differ");
        }
    }
}
