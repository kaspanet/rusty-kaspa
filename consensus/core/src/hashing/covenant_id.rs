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
/// public key (excluding the covenant binding itself to avoid self-reference);
/// any change to these yields a distinct covenant identifier.
pub fn covenant_id<'a>(
    outpoint: TransactionOutpoint,
    auth_outputs: impl ExactSizeIterator<Item = (u32, &'a TransactionOutput)>,
) -> Hash {
    let mut hasher = kaspa_hashes::CovenantID::new();
    hasher.update(outpoint.transaction_id).write_u32(outpoint.index).write_len(auth_outputs.len());
    for (index, output) in auth_outputs {
        hasher
            .write_u32(index)
            .write_u64(output.value)
            .write_u16(output.script_public_key.version())
            .write_var_bytes(output.script_public_key.script());
    }
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{CovenantBinding, ScriptPublicKey, TransactionId};
    use std::str::FromStr;

    struct Test {
        name: &'static str,
        outpoint: TransactionOutpoint,
        auth_outputs: Vec<(u32, TransactionOutput)>,
        expected_preimage: &'static str,
        expected_id: &'static str,
    }

    fn decode_hex(hex: &str) -> Vec<u8> {
        let mut bytes = vec![0u8; hex.len() / 2];
        faster_hex::hex_decode(hex.as_bytes(), &mut bytes).unwrap();
        bytes
    }

    fn spk(version: u16, script_hex: &str) -> ScriptPublicKey {
        ScriptPublicKey::from_vec(version, decode_hex(script_hex))
    }

    fn vectors() -> Vec<Test> {
        let tx_id = TransactionId::from_str("206a0c31dbe45384b86de451a177fd7c8e986d278f2a86724d941937443113ef").unwrap();
        vec![
            // KCC-23 binding fixture (kaspanet/kccs, kcc-0023/vectors/conformance.json, `binding.expected.covenant_id`)
            Test {
                name: "kcc-23 binding",
                outpoint: TransactionOutpoint::new(tx_id, 0),
                auth_outputs: vec![(0, TransactionOutput::new(90_000_000, spk(0, "51")))],
                expected_preimage: concat!(
                    "206a0c31dbe45384b86de451a177fd7c8e986d278f2a86724d941937443113ef", // outpoint tx id
                    "00000000",                                                         // outpoint index
                    "0100000000000000",                                                 // number of auth outputs
                    "00000000",                                                         // output index
                    "804a5d0500000000",                                                 // value
                    "0000",                                                             // spk version
                    "0100000000000000",                                                 // script length
                    "51",                                                               // script
                ),
                expected_id: "c86366cec738e9a1578b2d871ee493dcaabfd79156810f13f3b62429859e394e",
            },
            // No authorized outputs
            Test {
                name: "empty auth outputs",
                outpoint: TransactionOutpoint::new(tx_id, 0),
                auth_outputs: vec![],
                expected_preimage: concat!(
                    "206a0c31dbe45384b86de451a177fd7c8e986d278f2a86724d941937443113ef",
                    "00000000",
                    "0000000000000000",
                ),
                expected_id: "832bbbf167c8d8b8995887f31ac015e0ac95b994a2a0fca04732ca3c23e76241",
            },
            // Several outputs: non-contiguous indices, zero value, u64::MAX value, non-zero spk version, empty script
            Test {
                name: "multiple auth outputs",
                outpoint: TransactionOutpoint::new(tx_id, 258),
                auth_outputs: vec![
                    (1, TransactionOutput::new(1000, spk(0, "51"))),
                    (3, TransactionOutput::new(0, spk(1, ""))),
                    (
                        300,
                        TransactionOutput::new(
                            u64::MAX,
                            spk(0, "aa20000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f87"),
                        ),
                    ),
                ],
                expected_preimage: concat!(
                    "206a0c31dbe45384b86de451a177fd7c8e986d278f2a86724d941937443113ef",
                    "02010000",
                    "0300000000000000",
                    "01000000e8030000000000000000010000000000000051",
                    "03000000000000000000000001000000000000000000",
                    "2c010000ffffffffffffffff00002300000000000000",
                    "aa20000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f87",
                ),
                expected_id: "8071e66b8c7c02c75681ae4f62caf060856712e798a3bc44936073bbbea23d87",
            },
        ]
    }

    #[test]
    fn test_covenant_id_vectors() {
        for test in vectors() {
            let id = covenant_id(test.outpoint, test.auth_outputs.iter().map(|(i, o)| (*i, o)));
            assert_eq!(id, Hash::from_str(test.expected_id).unwrap(), "test '{}' failed", test.name);
        }
    }

    /// Pins the hash construction stated in KIP-20 §3: BLAKE2b with a 32-byte digest,
    /// keyed with the raw 10-byte tag `CovenantID` (no padding, salt or personalization),
    /// over the canonical encoding of §3.2.
    #[test]
    fn test_covenant_id_is_keyed_blake2b_256() {
        for test in vectors() {
            let preimage = decode_hex(test.expected_preimage);
            let direct = kaspa_hashes::blake2b_simd::Params::new().hash_length(32).key(b"CovenantID").hash(&preimage);
            assert_eq!(direct.as_bytes(), Hash::from_str(test.expected_id).unwrap().as_bytes(), "test '{}' failed", test.name);

            let id = covenant_id(test.outpoint, test.auth_outputs.iter().map(|(i, o)| (*i, o)));
            assert_eq!(id.as_bytes(), direct.as_bytes(), "test '{}' failed", test.name);
        }
    }

    #[test]
    fn test_covenant_id_excludes_covenant_binding() {
        for test in vectors() {
            let bound: Vec<_> = test
                .auth_outputs
                .iter()
                .map(|(i, o)| {
                    let binding = CovenantBinding::new(0, Hash::from_str(test.expected_id).unwrap());
                    (*i, TransactionOutput::with_covenant(o.value, o.script_public_key.clone(), Some(binding)))
                })
                .collect();
            let id = covenant_id(test.outpoint, bound.iter().map(|(i, o)| (*i, o)));
            assert_eq!(id, Hash::from_str(test.expected_id).unwrap(), "test '{}' failed", test.name);
        }
    }
}
