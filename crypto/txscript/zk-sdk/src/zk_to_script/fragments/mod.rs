//! Composable script-fragment building blocks for RISC0 zk-to-script locking
//! and spending scripts.
//!
//! These are free functions over a caller-owned [`ScriptBuilder`], deliberately
//! split into three concerns that are typically exercised by different software
//! at different times:
//!
//! - **witness push** (`push_*_witness`) — the transaction-builder side; maps a
//!   receipt to its on-stack bytes. These are **push-only** (data pushes, no
//!   opcodes) so they are safe to use inside a P2SH signature script, which the
//!   engine requires to be push-only.
//! - **journal production** — *not* owned by this module. The caller decides how
//!   the journal / journal_hash gets on the stack (a constant for a one-time
//!   covenant, or a runtime in-script computation in the redeem script for the
//!   general case).
//! - **verifier append** (`append_*_verifier`) — the covenant-author side;
//!   embeds the fixed program parameters and the zk precompile call. These emit
//!   opcodes and belong in the redeem script.
//!
//! The witness push and verifier append together produce exactly the stack shape
//! the precompile consumes, but the journal sits at a different point for each
//! proof system: groth16 expects `journal_hash` *under* the proof, so the caller
//! pushes it **before** [`push_r0_groth16_witness`]; succinct expects `journal`
//! *on top* of the receipt items, so the caller pushes it **after**
//! [`push_r0_succinct_witness`]. The `*_with_fixed_journal` helpers bake a
//! constant journal into the verifier for the one-time-covenant case (where it
//! is a redeem-script constant rather than a signature-script push). The pure
//! [`prepare`] tier exposes the receipt→bytes mapping without a [`ScriptBuilder`]
//! so a transaction builder can compute the bytes once and push them as plain
//! data.
//!
//! [`ScriptBuilder`]: kaspa_txscript::script_builder::ScriptBuilder

mod groth16;
mod prepare;
mod succinct;

pub use groth16::{append_r0_groth16_verifier, append_r0_groth16_verifier_with_fixed_journal, push_r0_groth16_witness};
pub use prepare::{SuccinctWitnessBytes, prepare_r0_groth16_proof, prepare_r0_succinct_witness};
pub use succinct::{append_r0_succinct_verifier, append_r0_succinct_verifier_with_fixed_journal, push_r0_succinct_witness};
