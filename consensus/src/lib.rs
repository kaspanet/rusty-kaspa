//! # Reachability and Storage Invariants
//!
//! This module relies on a set of structural invariants relating the different
//! block-related storage layers maintained by the node.
//!
//! ## Terminology
//!
//! Let:
//! - **B** be the set of blocks with a block body entry
//! - **R** be the set of blocks with a relations entry
//! - **C** be the set of blocks with a reachability entry
//! - **H** be the set of blocks with a header entry
//!
//! Let `pp` denote the current pruning point, and define:
//!
//! ```text
//! cut(pp) = { pp } ∪ (anticone(pp) ∩ past(virtual))
//! ```
//!
//! ## Conceptual Set Relationships
//!
//! Conceptually (up to transient noise during pruning), the following inclusion
//! relationships hold:
//!
//! ```text
//! B ⊆ R ⊆ C ⊆ H
//! ```
//!
//! In more concrete terms:
//!
//! - **B** consists of the main DAG area we actively process, roughly
//!   `past(virtual) \ past(pp)`.
//! - **R** extends **B** with additional blocks required for consensus operation,
//!   including a consecutive window below the pruning point, DAA windows of `cut(pp)`,
//!   and proof-level-0 blocks.
//! - **C** extends **R** with all higher proof levels.
//! - **H** extends **C** with headers of past pruning points, which are retained
//!   for historical and structural reasons but are not part of the reachable DAG.
//!
//! ## Implications for Code
//!
//! Examples of implications of the above invariants:
//!
//! - Any block inserted into the relations store (**R**) must reference only parents
//!   that are themselves already in **R**.
//! - The existence of reachability (**C**) or relations (**R**) data for a block
//!   implies the existence of a corresponding header entry (**H**), but not vice versa.
//!
//! Functions in this module assume and enforce these invariants. Callers are expected
//! to respect them as well.

pub mod config;
pub mod consensus;
pub mod constants;
pub mod errors;
pub mod model;
pub mod params;
pub mod pipeline;
pub mod processes;
pub mod test_helpers;
