use std::collections::HashMap;

use super::*;
use consensus_core::tx::TransactionOutpoint;
//TODO: explore potential optimization via custom TransactionOutpoint hasher for below,
//One possible implementation: u64 of transaction id xored with 4 bytes of transaction index.
pub type CompactUtxoCollection = HashMap<TransactionOutpoint, CompactUtxoEntry>;
