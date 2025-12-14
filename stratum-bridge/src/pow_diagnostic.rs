// Diagnostic test to find the PoW calculation bug
use kaspa_consensus_core::header::Header;
use kaspa_pow::State as PowState;
// Unused diagnostic imports removed - this module is for testing only
// use kaspa_hashes::{Hash, HasherBase, ProofOfWorkHash, BlockHash};
use num_bigint::BigUint;
use num_traits::ToPrimitive;

pub fn diagnose_pow_issue(header: &Header, nonce: u64) {
    tracing::debug!("\n========================================");
    tracing::debug!("PoW DIAGNOSTIC TEST");
    tracing::debug!("========================================");
    
    // Print header details
    tracing::debug!("\n[HEADER DETAILS]");
    tracing::debug!("  Version: {}", header.version);
    tracing::debug!("  Timestamp: {}", header.timestamp);
    tracing::debug!("  Bits: {} (0x{:08x})", header.bits, header.bits);
    tracing::debug!("  Nonce in header: {}", header.nonce);
    tracing::debug!("  Nonce to test: {}", nonce);
    
    // Calculate target from bits
    let exponent = (header.bits >> 24) as u32;
    let mantissa = header.bits & 0xFFFFFF;
    tracing::debug!("\n[TARGET CALCULATION]");
    tracing::debug!("  Exponent: {} (0x{:x})", exponent, exponent);
    tracing::debug!("  Mantissa: {} (0x{:06x})", mantissa, mantissa);
    tracing::debug!("  Shift: 8 * ({} - 3) = {} bits", exponent, 8 * (exponent - 3));
    
    let shift = if exponent > 3 { 8 * (exponent - 3) } else { 0 };
    let target = BigUint::from(mantissa) << shift;
    tracing::debug!("  Target: 0x{:064x}", target);
    tracing::debug!("  Target magnitude: {:.3e}", target.to_f64().unwrap_or(0.0));
    
    // Method 1: PowState with nonce already in header
    tracing::debug!("\n[METHOD 1: PowState with header nonce]");
    let mut header1 = header.clone();
    header1.nonce = nonce;
    let pow_state1 = PowState::new(&header1);
    let (passed1, value1) = pow_state1.check_pow(nonce);
    let pow_value1 = BigUint::from_bytes_be(&value1.to_be_bytes());
    tracing::debug!("  Result: {}", if passed1 { "PASS" } else { "FAIL" });
    tracing::debug!("  Pow value: 0x{:064x}", pow_value1);
    tracing::debug!("  Pow magnitude: {:.3e}", pow_value1.to_f64().unwrap_or(0.0));
    tracing::debug!("  Ratio to target: {:.2e}", pow_value1.to_f64().unwrap_or(0.0) / target.to_f64().unwrap_or(1.0));
    
    // Method 2: PowState with nonce=0 in header, passing nonce to check_pow
    tracing::debug!("\n[METHOD 2: PowState with nonce passed to check_pow]");
    let mut header2 = header.clone();
    header2.nonce = 0;  // Set to 0 first
    let pow_state2 = PowState::new(&header2);
    let (passed2, value2) = pow_state2.check_pow(nonce);  // Then pass nonce
    let pow_value2 = BigUint::from_bytes_be(&value2.to_be_bytes());
    tracing::debug!("  Result: {}", if passed2 { "PASS" } else { "FAIL" });
    tracing::debug!("  Pow value: 0x{:064x}", pow_value2);
    tracing::debug!("  Pow magnitude: {:.3e}", pow_value2.to_f64().unwrap_or(0.0));
    tracing::debug!("  Ratio to target: {:.2e}", pow_value2.to_f64().unwrap_or(0.0) / target.to_f64().unwrap_or(1.0));
    
    // Method 3: Test with nonce=0
    tracing::debug!("\n[METHOD 3: Test with nonce=0]");
    let mut header3 = header.clone();
    header3.nonce = 0;
    let pow_state3 = PowState::new(&header3);
    let (passed3, value3) = pow_state3.check_pow(0);
    let pow_value3 = BigUint::from_bytes_be(&value3.to_be_bytes());
    tracing::debug!("  Result: {}", if passed3 { "PASS" } else { "FAIL" });
    tracing::debug!("  Pow value: 0x{:064x}", pow_value3);
    tracing::debug!("  Pow magnitude: {:.3e}", pow_value3.to_f64().unwrap_or(0.0));
    
    // Method 4: Try minimum possible nonce
    tracing::debug!("\n[METHOD 4: Trying nonce=1]");
    let mut header4 = header.clone();
    header4.nonce = 1;
    let pow_state4 = PowState::new(&header4);
    let (passed4, value4) = pow_state4.check_pow(1);
    let pow_value4 = BigUint::from_bytes_be(&value4.to_be_bytes());
    tracing::debug!("  Result: {}", if passed4 { "PASS" } else { "FAIL" });
    tracing::debug!("  Pow value: 0x{:064x}", pow_value4);
    tracing::debug!("  Ratio to target: {:.2e}", pow_value4.to_f64().unwrap_or(0.0) / target.to_f64().unwrap_or(1.0));
    
    // Check if ANY nonce in a small range would pass
    tracing::debug!("\n[BRUTE FORCE TEST: First 1000 nonces]");
    let mut found_valid = false;
    for test_nonce in 0..1000 {
        let mut test_header = header.clone();
        test_header.nonce = test_nonce;
        let test_pow_state = PowState::new(&test_header);
        let (test_passed, test_value) = test_pow_state.check_pow(test_nonce);
        if test_passed {
            let test_pow_value = BigUint::from_bytes_be(&test_value.to_be_bytes());
            tracing::debug!("  ✓ FOUND VALID NONCE: {}", test_nonce);
            tracing::debug!("    Pow value: 0x{:064x}", test_pow_value);
            found_valid = true;
            break;
        }
    }
    
    if !found_valid {
        tracing::debug!("  ✗ No valid nonce found in range 0-999");
        tracing::debug!("  This confirms the issue: even with devnet difficulty, we can't find valid blocks");
    }
    
    // Statistical analysis
    tracing::debug!("\n[STATISTICAL ANALYSIS]");
    let target_f64 = target.to_f64().unwrap_or(1.0);
    let pow1_f64 = pow_value1.to_f64().unwrap_or(0.0);
    let factor = pow1_f64 / target_f64;
    let bits_off = factor.log2();
    tracing::debug!("  Factor off: {:.2e} ({:.1}x)", factor, factor);
    tracing::debug!("  Bits off: {:.2} bits", bits_off);
    tracing::debug!("  Expected probability: 1 in {:.2e}", 2.0_f64.powf(256.0) / target_f64);
    tracing::debug!("  Actual (if working): Should find valid block in ~{} hashes", (2.0_f64.powf(256.0) / target_f64) as u64);
    
    tracing::debug!("\n========================================");
    tracing::debug!("END DIAGNOSTIC");
    tracing::debug!("========================================\n");
}

