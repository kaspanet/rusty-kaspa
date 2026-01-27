use super::HasherExtensions;
use crate::{
    mass::transaction_estimated_serialized_size,
    tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput},
};
use kaspa_hashes::{Hash, Hasher, HasherBase, PayloadDigest};

bitflags::bitflags! {
    /// A bitmask defining which transaction fields we want to encode and which to ignore.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TxEncodingFlags: u8 {
        const FULL = 0;
        const EXCLUDE_SIGNATURE_SCRIPT = 1 << 0;
        const EXCLUDE_MASS_COMMIT = 1 << 1;
        const EXCLUDE_PAYLOAD = 1 << 2;
    }
}

/// Returns the transaction hash. Note that this is different than the transaction ID.
pub fn hash(tx: &Transaction) -> Hash {
    let mut hasher = kaspa_hashes::TransactionHash::new();
    write_transaction(&mut hasher, tx, TxEncodingFlags::FULL);
    hasher.finalize()
}

/// Returns the transaction hash pre-crescendo (which excludes the mass commitment)
pub fn hash_pre_crescendo(tx: &Transaction) -> Hash {
    let mut hasher = kaspa_hashes::TransactionHash::new();
    write_transaction(&mut hasher, tx, TxEncodingFlags::EXCLUDE_MASS_COMMIT);
    hasher.finalize()
}

/// Not intended for direct use by clients. Instead use `tx.id()`
pub fn id(tx: &Transaction) -> TransactionId {
    if tx.version == 0 { id_v0(tx) } else { id_v1(tx) }
}

pub fn id_v0(tx: &Transaction) -> TransactionId {
    let mut hasher = kaspa_hashes::TransactionID::new();
    write_transaction_v0_for_transaction_id(&mut hasher, tx);
    hasher.finalize()
}

fn write_transaction_v0_for_transaction_id<T: HasherBase>(hasher: &mut T, tx: &Transaction) {
    // Encode the transaction, replace signature script with an empty array, skip
    // sigop counts and mass commitment and hash the result.
    write_transaction(hasher, tx, TxEncodingFlags::EXCLUDE_SIGNATURE_SCRIPT | TxEncodingFlags::EXCLUDE_MASS_COMMIT)
}

/// Write the transaction into the provided hasher according to the encoding flags
fn write_transaction<T: HasherBase>(hasher: &mut T, tx: &Transaction, encoding_flags: TxEncodingFlags) {
    hasher.update(tx.version.to_le_bytes()).write_len(tx.inputs.len());
    for input in tx.inputs.iter() {
        // Write the tx input
        write_input(hasher, input, encoding_flags);
    }

    hasher.write_len(tx.outputs.len());
    for output in tx.outputs.iter() {
        // Write the tx output
        write_output(hasher, output, tx.version);
    }

    hasher.update(tx.lock_time.to_le_bytes()).update(tx.subnetwork_id).update(tx.gas.to_le_bytes());
    if !encoding_flags.contains(TxEncodingFlags::EXCLUDE_PAYLOAD) {
        hasher.write_var_bytes(&tx.payload);
    } else {
        hasher.write_var_bytes(&[]);
    };

    /*
       Design principles (mostly related to the new mass commitment field; see KIP-0009):
           1. The new mass field should not modify tx::id (since it is essentially a commitment by the miner re block space usage
              so there is no need to modify the id definition which will require wide-spread changes in ecosystem software).
           2. Coinbase tx hash should ideally remain unchanged

       Solution:
           1. Hash the mass field only for tx::hash
           2. Hash the mass field only if mass > 0
           3. Require in consensus that coinbase mass == 0

       This way we have:
           - Unique commitment for tx::hash per any possible mass value (with only zero being a no-op)
           - tx::id remains unmodified
           - Coinbase tx hash remains unchanged
    */

    if !encoding_flags.contains(TxEncodingFlags::EXCLUDE_MASS_COMMIT) {
        let mass = tx.mass();
        if tx.version < 1 {
            if mass > 0 {
                hasher.update(mass.to_le_bytes());
            }
        } else {
            // In order make the encoding unambiguous and invertible in case of future additional fields, for version >= 1 we always include the mass field
            hasher.update(mass.to_le_bytes());
        }
    }
}

#[inline(always)]
fn write_input<T: HasherBase>(hasher: &mut T, input: &TransactionInput, encoding_flags: TxEncodingFlags) {
    write_outpoint(hasher, &input.previous_outpoint);
    if !encoding_flags.contains(TxEncodingFlags::EXCLUDE_SIGNATURE_SCRIPT) {
        hasher.write_var_bytes(input.signature_script.as_slice()).update([input.sig_op_count]);
    } else {
        hasher.write_var_bytes(&[]);
    }
    hasher.update(input.sequence.to_le_bytes());
}

#[inline(always)]
fn write_outpoint<T: HasherBase>(hasher: &mut T, outpoint: &TransactionOutpoint) {
    hasher.update(outpoint.transaction_id).update(outpoint.index.to_le_bytes());
}

#[inline(always)]
fn write_output<T: HasherBase>(hasher: &mut T, output: &TransactionOutput, version: u16) {
    hasher
        .update(output.value.to_le_bytes())
        .update(output.script_public_key.version().to_le_bytes())
        .write_var_bytes(output.script_public_key.script());

    if version >= 1 {
        hasher.write_bool(output.covenant.is_some());
        if let Some(covenant) = &output.covenant {
            hasher.write_u16(covenant.authorizing_input);
            hasher.update(covenant.covenant_id);
        }
    }
}

struct PreimageHasher {
    buff: Vec<u8>,
}

impl HasherBase for PreimageHasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        self.buff.extend_from_slice(data.as_ref());
        self
    }
}

/// Serializes the transaction for v0 TxID preimage (excluding signature scripts).
pub fn transaction_v0_id_preimage(tx: &Transaction) -> Vec<u8> {
    assert_eq!(tx.version, 0);
    let mut hasher = PreimageHasher { buff: Vec::with_capacity(transaction_estimated_serialized_size(tx) as usize) };
    write_transaction_v0_for_transaction_id(&mut hasher, tx);
    hasher.buff
}

/// Precomputed hash digest for an empty payload using `PayloadDigest`.
const ZERO_PAYLOAD_DIGEST: Hash = Hash::from_bytes([
    156, 12, 162, 172, 180, 94, 146, 255, 230, 206, 180, 174, 41, 24, 139, 53, 200, 45, 150, 118, 205, 211, 206, 6, 127, 214, 204,
    195, 10, 156, 74, 56,
]);

/// Computes the Transaction ID for a version 1 transaction.
pub fn id_v1(tx: &Transaction) -> TransactionId {
    let payload_digest = payload_digest(&tx.payload);
    let rest_digest = {
        let mut hasher = kaspa_hashes::TransactionRest::new();
        write_transaction(
            &mut hasher,
            tx,
            TxEncodingFlags::EXCLUDE_PAYLOAD | TxEncodingFlags::EXCLUDE_SIGNATURE_SCRIPT | TxEncodingFlags::EXCLUDE_MASS_COMMIT,
        );
        hasher.finalize()
    };

    let mut hasher = kaspa_hashes::TransactionV1Id::new();
    hasher.update(payload_digest).update(rest_digest);
    hasher.finalize()
}

/// Computes the digest of the transaction payload using `PayloadDigest` hasher.
pub fn payload_digest(payload: &[u8]) -> Hash {
    if payload.is_empty() { ZERO_PAYLOAD_DIGEST } else { PayloadDigest::hash(payload) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        subnets::{self, SubnetworkId},
        tx::{ScriptPublicKey, scriptvec},
    };
    use std::str::FromStr;

    #[test]
    fn test_zero_payload_digest() {
        assert_eq!(ZERO_PAYLOAD_DIGEST, PayloadDigest::hash([]));
    }

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
                tx: Transaction::new(0, Vec::new(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
                expected_id: "2c18d5e59ca8fc4c23d9560da3bf738a8f40935c11c162017fbf2c907b7e665c",
                expected_hash: "c9e29784564c269ce2faaffd3487cb4684383018ace11133de082dce4bb88b0b",
            },
        ];

        let inputs = vec![TransactionInput::new(TransactionOutpoint::new(Hash::from_u64_word(0), 2), vec![1, 2], 7, 5)];

        // Test #2
        tests.push(Test {
            tx: Transaction::new(0, inputs.clone(), Vec::new(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
            expected_id: "b2d65ae36e123eb73f253176d7234a57656b84d0d60b9fc746ab0d0f085c9cc7",
            expected_hash: "7d9f7cfdd77f236a41895ac5cdda2fa42f7122964ba995fdfacebce54efad7e8",
        });

        let outputs = vec![TransactionOutput::new(1564, ScriptPublicKey::new(7, scriptvec![1, 2, 3, 4, 5]))];

        // Test #3
        tests.push(Test {
            tx: Transaction::new(0, inputs.clone(), outputs.clone(), 0, SubnetworkId::from_byte(0), 0, Vec::new()),
            expected_id: "67289b12146d1b5ef384332137399791a5cfe89506ff31688b0d95ae821d0a0c",
            expected_hash: "492279c0ed5018aa00b0b2d42c1c42350285f2e689236a81829edaf818e30fdb",
        });

        // Test #4
        tests.push(Test {
            tx: Transaction::new(0, inputs, outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new()),
            expected_id: "7cd34b788d7d230970d4bfd955c34c5abc49e3bcdd5adb03a77bb71d05554401",
            expected_hash: "de319664ee9f4197e89be0d0e08b2b6cac110efc2cf107de1fbc6bd2ce29d545",
        });

        let inputs = vec![TransactionInput::new(
            TransactionOutpoint::new(Hash::from_str("59b3d6dc6cdc660c389c3fdb5704c48c598d279cdf1bab54182db586a4c95dd5").unwrap(), 2),
            vec![1, 2],
            7,
            5,
        )];

        // Test #5
        tests.push(Test {
            tx: Transaction::new(0, inputs.clone(), outputs.clone(), 54, SubnetworkId::from_byte(0), 3, Vec::new()),
            expected_id: "c9dd78e818445f617a28348d6db752142e2fab440effa58140ad2773e638b628",
            expected_hash: "1be9978bcab9424f15adac6fca0a64c3f56344a7cd0ec92a225496e19a0d122c",
        });

        // Test #6
        tests.push(Test {
            // Valid coinbase transactions have no inputs.
            tx: Transaction::new(0, vec![], outputs.clone(), 54, subnets::SUBNETWORK_ID_COINBASE, 3, Vec::new()),
            expected_id: "2578783ec93c3a02414a228e10b1b5af298623254775f972f97df08d4ec28c8f",
            expected_hash: "dffa96c75ef9d17520991fc6d88813531e230488e75b65f65ce958f2d54d2451",
        });

        // Test #7
        tests.push(Test {
            tx: Transaction::new(0, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, Vec::new()),
            expected_id: "3f6cea6d7ac8f6b2f86209fa748ea0ef5a1d5d380d43b79e77d52e770bb9a7b9",
            expected_hash: "9abf01c6c312dd984ff19c23bec85e8678e6ea34041fe3c5de52fd9344adac63",
        });

        // Test #8, same as 7 but with a non-zero payload. The test checks id and hash are affected by payload change
        tests.push(Test {
            tx: Transaction::new(0, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, vec![1, 2, 3]),
            expected_id: "4acda997dfb31c6518224c9ac00d0777fc7cbecdab461be3c0816b1cba19a056",
            expected_hash: "f0bb137ed71a91445ddf9224c76f755153a296eeb4fdc29b8393ddd81bf34ce6",
        });

        // Test #9, same as 7 but with a non-zero payload. The test checks only hash is affected by mass commitment
        tests.push(Test {
            tx: Transaction::new_with_mass(
                0,
                inputs.clone(),
                outputs.clone(),
                54,
                subnets::SUBNETWORK_ID_REGISTRY,
                3,
                vec![1, 2, 3],
                5,
            ),
            expected_id: "4acda997dfb31c6518224c9ac00d0777fc7cbecdab461be3c0816b1cba19a056",
            expected_hash: "ced89bbf642cda42d29d9518d16e35cbbf85d10e1ab106b7dc2e0a821308ac91",
        });

        // // Test #10, same as 9 with different version and checks it affects id and hash
        // tests.push(Test {
        //     tx: Transaction::new(1, inputs.clone(), outputs.clone(), 54, subnets::SUBNETWORK_ID_REGISTRY, 3, vec![1, 2, 3]),
        //     expected_id: "9ec65c816b495e7da8f88c6d261af00b7bca45e398a4373f92eb665e7d7cf79d",
        //     expected_hash: "6c8fed2799b478667914748b9c76da576fc18b44ce87c6ebc01c01705f13f3e3",
        // });

        for (i, test) in tests.iter().enumerate() {
            assert_eq!(test.tx.id(), Hash::from_str(test.expected_id).unwrap(), "transaction id failed for test {}", i + 1);
            assert_eq!(hash(&test.tx), Hash::from_str(test.expected_hash).unwrap(), "transaction hash failed for test {}", i + 1);

            let preimage = transaction_v0_id_preimage(&test.tx);
            let mut hasher = kaspa_hashes::TransactionID::new();
            hasher.update(&preimage);
            let preimage_hash = hasher.finalize();
            assert_eq!(preimage_hash, test.tx.id(), "transaction id preimage failed for test {}", i + 1);
        }

        // Avoid compiler warnings on the last clone
        drop(inputs);
        drop(outputs);

        // TODO(pre-covpp) add tests for v1 hashes
    }
}
