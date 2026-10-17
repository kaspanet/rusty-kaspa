//! Checks that `construct_uint!` expands cleanly in a downstream crate.
//!
//! This file compiles as its own crate with no `extern crate alloc`, no
//! feature declarations of its own, and no dependencies beyond kaspa-math's
//! public surface, so any unqualified `alloc::`/`kaspa_utils` path or
//! caller-side `cfg(feature)` emitted by the macro fails to build here.
//! Not covered: paths that resolve from the std prelude (`String`,
//! `format!`), since a test target always links std.

use kaspa_math::{Uint256, construct_uint};

construct_uint!(Uint512, 8);

#[test]
fn alloc_backed_api_is_caller_hygienic() {
    assert_eq!(Uint512::from_u64(0x0102).to_be_bytes_var(), vec![0x01, 0x02]);
    assert_eq!(Uint256::from_u64(0x0102).to_be_bytes_var(), vec![0x01, 0x02]);
}

#[test]
fn hex_impls_are_generated() {
    use kaspa_utils::hex::ToHex;

    let hex = Uint512::from_u64(0xff).to_hex();
    assert_eq!(Uint512::from_hex(&hex).unwrap(), Uint512::from_u64(0xff));
}

#[cfg(feature = "std")]
#[test]
fn memsize_impl_follows_kaspa_math_features() {
    use kaspa_utils::mem_size::MemSizeEstimator;

    assert_eq!(Uint256::from_u64(1).estimate_mem_units(), 1);
    assert_eq!(Uint512::from_u64(1).estimate_mem_units(), 1);
}

/// Compile-time check only: wasm-bindgen entry points are inert on non-wasm hosts.
#[cfg(feature = "wasm32-sdk")]
#[allow(dead_code)]
fn wasm_conversion_api_is_generated(value: &Uint512) {
    let _ = value.as_bigint();
    let _ = Uint512::from_u64(1).to_bigint();
}
