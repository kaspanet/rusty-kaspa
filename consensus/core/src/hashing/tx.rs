use super::HasherExtensions;
use crate::tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput};
use hashes::{Hash, Hasher};

/// A bitmask defining which transaction fields we
/// want to encode and which to ignore.
type TxEncodingFlags = u8;

pub const TX_ENCODING_FULL: TxEncodingFlags = 0;
pub const TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT: TxEncodingFlags = 1;

/// Returns the transaction hash. Note that this is different than the transaction ID.
pub fn hash(tx: &Transaction) -> Hash {
    let mut hasher = hashes::TransactionHash::new();
    write_transaction(&mut hasher, tx, TX_ENCODING_FULL);
    hasher.finalize()
}

/// Not intended for direct use by clients. Instead use `tx.id()`
pub(crate) fn id(tx: &Transaction) -> TransactionId {
    // Encode the transaction, replace signature script with zeroes, cut off
    // payload and hash the result.

    let encoding_flags = if tx.is_coinbase() { TX_ENCODING_FULL } else { TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT };
    let mut hasher = hashes::TransactionID::new();
    write_transaction(&mut hasher, tx, encoding_flags);
    hasher.finalize()
}

/// Write the transaction into the provided hasher according to the encoding flags
fn write_transaction<T: Hasher>(hasher: &mut T, tx: &Transaction, encoding_flags: TxEncodingFlags) {
    hasher.update(tx.version.to_le_bytes()).write_len(tx.inputs.len());
    for input in tx.inputs.iter() {
        // Write the tx input
        write_input(hasher, input, encoding_flags);
    }

    hasher.write_len(tx.outputs.len());
    for output in tx.outputs.iter() {
        // Write the tx output
        write_output(hasher, output);
    }

    hasher.update(tx.lock_time.to_le_bytes()).update(&tx.subnetwork_id).update(tx.gas.to_le_bytes()).write_var_bytes(&tx.payload);
}

#[inline(always)]
fn write_input<T: Hasher>(hasher: &mut T, input: &TransactionInput, encoding_flags: TxEncodingFlags) {
    write_outpoint(hasher, &input.previous_outpoint);
    if encoding_flags & TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT != TX_ENCODING_EXCLUDE_SIGNATURE_SCRIPT {
        hasher.write_var_bytes(input.signature_script.as_slice()).update([input.sig_op_count]);
    } else {
        hasher.write_var_bytes(&[]);
    }
    hasher.update(input.sequence.to_le_bytes());
}

#[inline(always)]
fn write_outpoint<T: Hasher>(hasher: &mut T, outpoint: &TransactionOutpoint) {
    hasher.update(outpoint.transaction_id).update(outpoint.index.to_le_bytes());
}

#[inline(always)]
fn write_output<T: Hasher>(hasher: &mut T, output: &TransactionOutput) {
    hasher
        .update(output.value.to_le_bytes())
        .update(output.script_public_key.version.to_le_bytes())
        .write_var_bytes(&output.script_public_key.script);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::{self, SubnetworkId},
        tx::ScriptPublicKey,
    };
    use std::{str::FromStr, sync::Arc};

    #[test]
    fn test_transaction_hashing() {
        struct Test {
            tx: Transaction,
            expected_id: &'static str,
            expected_hash: &'static str,
        }

        let mut tests = vec![
            // Test #1
            Test {
                tx: Transaction::new(0, Vec::new(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new(), 0),
                expected_id: "2c18d5e59ca8fc4c23d9560da3bf738a8f40935c11c162017fbf2c907b7e665c",
                expected_hash: "c9e29784564c269ce2faaffd3487cb4684383018ace11133de082dce4bb88b0b",
            },
        ];

        let inputs =
            vec![Arc::new(TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(0), 2), vec![1, 2], 7, 5, None))];

        // Test #2
        tests.push(Test {
            tx: Transaction::new(1, inputs.clone(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new(), 0),
            expected_id: "dafa415216d26130a899422203559c809d3efe72e20d48505fb2f08787bc4f49",
            expected_hash: "e4045023768d98839c976918f80c9419c6a93003724eda97f7c61a5b68de851b",
        });

        let outputs = vec![Arc::new(TransactionOutput::new(1564, Arc::new(ScriptPublicKey::new(vec![1, 2, 3, 4, 5], 7))))];

        // Test #3
        tests.push(Test {
            tx: Transaction::new(1, inputs.clone(), outputs.clone(), 0, SubnetworkId::from_byte(0), 0, Vec::new(), 0),
            expected_id: "d1cd9dc1f26955832ccd12c27afaef4b71443aa7e7487804baf340952ca927e5",
            expected_hash: "e5523c70f6b986cad9f6959e63f080e6ac5f93bc2a9e0e01a89ca9bf6908f51c",
        });

        // Test #4
        tests.push(Test {
            tx: Transaction::new(2, inputs, outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new(), 4),
            expected_id: "59b3d6dc6cdc660c389c3fdb5704c48c598d279cdf1bab54182db586a4c95dd5",
            expected_hash: "b70f2f14c2f161a29b77b9a78997887a8e727bb57effca38cd246cb270b19cd5",
        });

        let inputs = vec![Arc::new(TransactionInput::new(
            TransactionOutpoint::new(Hash::from_str("59b3d6dc6cdc660c389c3fdb5704c48c598d279cdf1bab54182db586a4c95dd5").unwrap(), 2),
            vec![1, 2],
            7,
            5,
            None,
        ))];

        // Test #5
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new(), 4),
            expected_id: "9d106623860567915b19cea33af486286a31b4bfc68627c6d4d377287afb40ad",
            expected_hash: "cd575e69fbf5f97fbfd4afb414feb56f8463b3948d6ac30f0ecdd9622672fab9",
        });

        // Test #6
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_COINBASE, 3, Vec::new(), 4),
            expected_id: "3fad809b11bd5a4af027aa4ac3fbde97e40624fd40965ba3ee1ee1b57521ad10",
            expected_hash: "b4eb5f0cab5060bf336af5dcfdeb2198cc088b693b35c87309bd3dda04f1cfb9",
        });

        // Test #7
        tests.push(Test {
            tx: Transaction::new(2, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, Vec::new(), 4),
            expected_id: "c542a204ab9416df910b01540b0c51b85e6d4e1724e081e224ea199a9e54e1b3",
            expected_hash: "31da267d5c34f0740c77b8c9ebde0845a01179ec68074578227b804bac306361",
        });

        for (i, test) in tests.iter().enumerate() {
            assert_eq!(test.tx.id(), Hash::from_str(test.expected_id).unwrap(), "transaction id failed for test {}", i + 1);
            assert_eq!(hash(&test.tx), Hash::from_str(test.expected_hash).unwrap(), "transaction hash failed for test {}", i + 1);
        }

        // Avoid compiler warnings on the last clone
        drop(inputs);
        drop(outputs);
    }
}
