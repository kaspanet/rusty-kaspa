//! # kaspa-seq-commit — KIP-0021 Sequencing Commitment
//!
//! Types and hash functions for KIP-0021 lane-based partitioned
//! sequencing commitments.
//!
//! ## Feature flags
//!
//! - **`std`** (default) — enables state management and collection.
//! - Without `std` — only types and hashing functions are available.

#![no_std]

extern crate alloc;
extern crate core;

pub mod hashing;
pub mod types;
