use kaspa_hashes::Hash;

use crate::BlueWorkType;

/// A data structure representing a unique approximate identifier for virtual state. Note that it is approximate
/// in the sense that in rare cases a slightly different virtual state might produce the same identifier,
/// hence it should be used for cache-like heuristics only
#[derive(PartialEq)]
pub struct VirtualStateApproxId {
    pub daa_score: u64,
    pub blue_work: BlueWorkType,
    pub sink: Hash,
}
