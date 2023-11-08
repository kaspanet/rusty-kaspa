use kaspa_hashes::Hash;

use crate::BlueWorkType;

/// An opaque data structure representing a unique approximate identifier for virtual state. Note that it is
/// approximate in the sense that in rare cases a slightly different virtual state might produce the same identifier,
/// hence it should be used for cache-like heuristics only
#[derive(PartialEq)]
pub struct VirtualStateApproxId {
    daa_score: u64,
    blue_work: BlueWorkType,
    sink: Hash,
}

impl VirtualStateApproxId {
    pub fn new(daa_score: u64, blue_work: BlueWorkType, sink: Hash) -> Self {
        Self { daa_score, blue_work, sink }
    }
}
