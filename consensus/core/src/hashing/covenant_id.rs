use kaspa_hashes::{Hash, HasherBase};

use crate::tx::TransactionOutpoint;

pub fn covenant_id(outpoint: TransactionOutpoint) -> Hash {
    let mut hasher = kaspa_hashes::CovenantID::new();
    hasher.update(outpoint.transaction_id).update(outpoint.index.to_le_bytes());
    hasher.finalize()
}
