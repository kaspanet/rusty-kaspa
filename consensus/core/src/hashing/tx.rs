use crate::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput};
use hashes::{Hash, Hasher};

/// A bitmask defining which transaction fields we
/// want to encode and which to ignore.
type TxEncodingFlags = u8;

pub const TX_ENCODING_FULL: TxEncodingFlags = 0;
pub const TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT: TxEncodingFlags = 1;

/// Returns the transaction hash. Note that this is different than the transaction ID.
pub fn transaction_hash(tx: &Transaction) -> Hash {
    let mut hasher = hashes::TransactionHash::new();
    write_transaction(&mut hasher, tx, TX_ENCODING_FULL);
    hasher.finalize()
}

/// Not intended for direct use by clients. Instead use `tx.id()`
pub(crate) fn transaction_id(tx: &Transaction) -> TransactionId {
    // Encode the transaction, replace signature script with zeroes, cut off
    // payload and hash the result.

    let encoding_flags = if tx.is_coinbase() { TX_ENCODING_FULL } else { TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT };
    let mut hasher = hashes::TransactionID::new();
    write_transaction(&mut hasher, tx, encoding_flags);
    hasher.finalize()
}

/// Write the transaction into the provided hasher according to the encoding flags
fn write_transaction<T: Hasher>(hasher: &mut T, tx: &Transaction, encoding_flags: TxEncodingFlags) {
    hasher
        .update(tx.version.to_le_bytes())
        .update((tx.inputs.len() as u64).to_le_bytes());
    for input in tx.inputs.iter() {
        // Write the tx input
        write_input(hasher, input, encoding_flags);
    }

    hasher.update((tx.outputs.len() as u64).to_le_bytes());
    for output in tx.outputs.iter() {
        // Write the tx output
        write_output(hasher, output);
    }

    hasher
        .update(tx.lock_time.to_le_bytes())
        .update(&tx.subnetwork_id)
        .update(tx.gas.to_le_bytes());

    write_var_bytes(hasher, &tx.payload);
}

#[inline(always)]
fn write_input<T: Hasher>(hasher: &mut T, input: &TransactionInput, encoding_flags: TxEncodingFlags) {
    write_outpoint(hasher, &input.previous_outpoint);
    if encoding_flags & TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT != TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT {
        write_var_bytes(hasher, input.signature_script.as_slice());
    } else {
        write_var_bytes(hasher, &[]);
    }
    hasher.update(input.sequence.to_le_bytes());
}

#[inline(always)]
fn write_outpoint<T: Hasher>(hasher: &mut T, outpoint: &TransactionOutpoint) {
    hasher
        .update(outpoint.transaction_id)
        .update(outpoint.index.to_le_bytes());
}

#[inline(always)]
fn write_output<T: Hasher>(hasher: &mut T, output: &TransactionOutput) {
    hasher
        .update(output.value.to_le_bytes())
        .update(output.script_public_key.version.to_le_bytes())
        .update(&output.script_public_key.script);
}

#[inline(always)]
fn write_var_bytes<T: Hasher>(hasher: &mut T, bytes: &[u8]) {
    hasher
        .update((bytes.len() as u64).to_le_bytes())
        .update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::SubnetworkId,
        tx::{ScriptPublicKey, UtxoEntry},
    };
    use std::{str::FromStr, sync::Arc};

    #[test]
    fn test_transaction_id() {
        let tx = Transaction::new(0, Vec::new(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new(), 0, 0);

        let actual = tx.id();
        let expected = Hash::from_str("2c18d5e59ca8fc4c23d9560da3bf738a8f40935c11c162017fbf2c907b7e665c").unwrap();

        println!("{}", actual);
        assert_eq!(actual, expected);

        let inputs = vec![Arc::new(TransactionInput::new(
            TransactionOutpoint::new(Hash::from_u64_word(0), 2),
            vec![1, 2],
            7,
            5,
            UtxoEntry::new(0, Arc::new(ScriptPublicKey::new(Vec::new(), 0)), 0, false),
        ))];

        let tx = Transaction::new(1, inputs, Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new(), 0, 0);
        let actual = tx.id();
        let expected = Hash::from_str("dafa415216d26130a899422203559c809d3efe72e20d48505fb2f08787bc4f49").unwrap();
        println!("{}", actual);
        assert_eq!(actual, expected);
    }
}
