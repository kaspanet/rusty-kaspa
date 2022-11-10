use consensus_core::tx::{PopulatedTransaction};
use hashes::{Hash, HasherBase, TransactionSigningHash};

fn previous_output_hash(tx: &PopulatedTransaction) -> Hash {
    // TODO: is anyone can pay by hashtype
    // TODO: cache values
    let mut hasher = TransactionSigningHash::new();
    for ref input in tx.tx.inputs.clone() {
        hasher.update(input.previous_outpoint.transaction_id.as_bytes());
        hasher.update(input.previous_outpoint.index.to_le_bytes());
    }
    hasher.finalize()
}

pub(crate) fn signature_hash(tx: &PopulatedTransaction) -> Hash {
    let mut hasher = TransactionSigningHash::new();
    hasher.update(tx.tx.version.to_le_bytes());
    hasher.update(previous_output_hash(tx));
    todo!()

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_hash() {

    }
}