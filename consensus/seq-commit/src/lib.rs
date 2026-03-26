//! # kaspa-seq-commit — Sequencing Commitment
//!
//! Types and hash functions for lane-based partitioned
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
pub mod verify;
