use kaspa_hashes::{Hash, HasherBase};

use crate::{
    hashing::HasherExtensions,
    tx::{TransactionOutpoint, TransactionOutput},
};

/// Computes the covenant identifier from the genesis outpoint and its authorized outputs.
///
/// The genesis outpoint serves as a globally unique anchor; the authorized outputs
/// define the covenant's initial rules and state.
///
/// The identifier incorporates each authorized output's index, value, and script
/// public key; any change to these yields a distinct covenant identifier.
pub fn covenant_id<'a>(
    outpoint: TransactionOutpoint,
    auth_outputs: impl ExactSizeIterator<Item = (u32, &'a TransactionOutput)>,
) -> Hash {
    let mut hasher = kaspa_hashes::CovenantID::new();
    hasher.update(outpoint.transaction_id).update(outpoint.index.to_le_bytes());
    hasher.write_len(auth_outputs.len());
    for (index, output) in auth_outputs {
        hasher
            .write_u32(index)
            .update(output.value.to_le_bytes())
            .update(output.script_public_key.version().to_le_bytes())
            .write_var_bytes(output.script_public_key.script());
    }
    hasher.finalize()
}
