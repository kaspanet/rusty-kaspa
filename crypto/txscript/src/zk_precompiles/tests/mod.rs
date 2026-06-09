pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{
        Groth16Fields, R0Fields, build_groth_script, build_groth_script_from_fields, build_stark_script, execute_p2sh_script,
        execute_zk_script, load_groth_fields, load_stark_fields,
    };
    use crate::{
        caches::Cache,
        get_zk_script_units_upper_bound,
        zk_precompiles::{groth16::Groth16Error, tags::ZkTag},
    };
    use ark_bn254::{Bn254, G1Affine};
    use ark_groth16::{Proof, VerifyingKey};
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };
    use kaspa_txscript_errors::TxScriptError;
    use risc0_circuit_recursion::CircuitImpl;
    use risc0_zkp::adapter::CircuitInfo;

    fn r0_script_with_seal(seal: &[u8]) -> Vec<u8> {
        let mut fields = R0Fields::from_fixture();
        fields.seal = seal.to_vec();
        fields.script()
    }

    fn r0_script_with_control_digests(control_digests: &[u8]) -> Vec<u8> {
        let mut fields = R0Fields::from_fixture();
        fields.control_digests = control_digests.to_vec();
        fields.script()
    }

    fn r0_script_with_control_id(control_id: &[u8]) -> Vec<u8> {
        let mut fields = R0Fields::from_fixture();
        fields.control_id = control_id.to_vec();
        fields.script()
    }

    fn words_to_le_bytes(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }

    fn expect_r0_receipt_format_err(result: Result<(), TxScriptError>, case: &str) {
        match result {
            Err(TxScriptError::ZkIntegrity(e)) if e == "R0: invalid receipt format" => {}
            Err(e) => panic!("{case}: expected R0 receipt format error, got {e:?}"),
            Ok(_) => panic!("{case}: expected R0 receipt format error, got success"),
        }
    }

    #[derive(Copy, Clone)]
    enum ExpectedZkError {
        Exact(&'static str),
        StartsWith(&'static str),
        Groth16ArityMismatch,
    }

    type R0FailureCase = (&'static str, fn(&mut R0Fields), ExpectedZkError);
    type Groth16FailureCase = (&'static str, fn(&mut Groth16Fields), ExpectedZkError);

    fn expect_zk_err(result: Result<(), TxScriptError>, case: &str, expected: ExpectedZkError) {
        match (result, expected) {
            (Err(TxScriptError::ZkIntegrity(e)), ExpectedZkError::Exact(expected)) if e == expected => {}
            (Err(TxScriptError::ZkIntegrity(e)), ExpectedZkError::StartsWith(expected)) if e.starts_with(expected) => {}
            (Err(TxScriptError::ZkIntegrity(e)), ExpectedZkError::Groth16ArityMismatch)
                if e == Groth16Error::ArkR1CS(ark_relations::gr1cs::SynthesisError::ArityMismatch).to_string() => {}
            (Err(e), ExpectedZkError::Exact(expected)) => panic!("{case}: expected ZkIntegrity({expected:?}), got {e:?}"),
            (Err(e), ExpectedZkError::StartsWith(expected)) => {
                panic!("{case}: expected ZkIntegrity prefix {expected:?}, got {e:?}")
            }
            (Err(e), ExpectedZkError::Groth16ArityMismatch) => panic!("{case}: expected Groth16 arity mismatch, got {e:?}"),
            (Ok(()), _) => panic!("{case}: expected ZkIntegrity error, got success"),
        }
    }

    fn set_seal_word(seal: &mut [u8], index: usize, word: u32) {
        let start = index * core::mem::size_of::<u32>();
        seal[start..start + core::mem::size_of::<u32>()].copy_from_slice(&word.to_le_bytes());
    }

    fn truncate_one_digest(bytes: &mut Vec<u8>) {
        bytes.truncate(bytes.len() - risc0_zkp::core::digest::DIGEST_BYTES);
    }

    fn expect_groth16_arity_mismatch(result: Result<(), TxScriptError>, case: &str) {
        let expected = Groth16Error::ArkR1CS(ark_relations::gr1cs::SynthesisError::ArityMismatch).to_string();
        match result {
            Err(TxScriptError::ZkIntegrity(e)) if e == expected => {}
            Err(e) => panic!("{case}: expected Groth16 arity mismatch, got {e:?}"),
            Ok(_) => panic!("{case}: expected Groth16 arity mismatch, got success"),
        }
    }

    fn serialize_vk(vk: &VerifyingKey<Bn254>) -> Vec<u8> {
        let mut bytes = Vec::new();
        vk.serialize_compressed(&mut bytes).expect("serialize VK");
        bytes
    }

    fn serialize_proof(proof: &Proof<Bn254>) -> Vec<u8> {
        let mut bytes = Vec::new();
        proof.serialize_compressed(&mut bytes).expect("serialize proof");
        bytes
    }

    fn set_empty_gamma_abc(fields: &mut Groth16Fields) {
        let mut vk = VerifyingKey::<Bn254>::deserialize_compressed(&*fields.vk).expect("fixture VK must deserialize");
        vk.gamma_abc_g1.clear();
        fields.vk = serialize_vk(&vk);
    }

    fn set_parseable_wrong_vk(fields: &mut Groth16Fields) {
        let mut vk = VerifyingKey::<Bn254>::deserialize_compressed(&*fields.vk).expect("fixture VK must deserialize");
        vk.alpha_g1 = G1Affine::default();
        fields.vk = serialize_vk(&vk);
    }

    fn set_parseable_wrong_proof(fields: &mut Groth16Fields) {
        let mut proof = Proof::<Bn254>::deserialize_compressed(&*fields.proof).expect("fixture proof must deserialize");
        proof.a = G1Affine::default();
        fields.proof = serialize_proof(&proof);
    }

    #[test]
    fn test_groth16_fast() {
        let script = build_groth_script();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();

        // Verify ZK static cost estimation formula
        let spk = ScriptPublicKey::from_vec(0, script);
        let estimated = get_zk_script_units_upper_bound::<PopulatedTransaction, SigHashReusedValuesUnsync>(&[], &spk);
        let expected = ZkTag::Groth16.cost();
        assert_eq!(estimated, expected);
    }

    #[test]
    fn verify_groth16_empty_gamma_abc_rejected() {
        let (vk_bytes, proof, inputs) = load_groth_fields();
        let mut vk = VerifyingKey::<ark_bn254::Bn254>::deserialize_compressed(&*vk_bytes).unwrap();
        vk.gamma_abc_g1.clear();

        let mut malformed_vk = Vec::new();
        vk.serialize_compressed(&mut malformed_vk).unwrap();
        let script = build_groth_script_from_fields(&malformed_vk, &proof, &inputs);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_zk_script(&script, &cache, &reused_values).expect_err("malformed verifying key should fail");
    }

    #[test]
    fn verify_groth16_missing_public_input_rejected() {
        let (vk, proof, mut inputs) = load_groth_fields();
        inputs.pop();
        let script = build_groth_script_from_fields(&vk, &proof, &inputs);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        expect_groth16_arity_mismatch(execute_zk_script(&script, &cache, &reused_values), "missing public input");
    }

    #[test]
    fn verify_groth16_extra_public_input_rejected() {
        let (vk, proof, mut inputs) = load_groth_fields();
        inputs.push(inputs[0].clone());
        let script = build_groth_script_from_fields(&vk, &proof, &inputs);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        expect_groth16_arity_mismatch(execute_zk_script(&script, &cache, &reused_values), "extra public input");
    }

    #[test]
    fn verify_groth16_failure_matrix() {
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        let cases: &[Groth16FailureCase] = &[
            (
                "missing public input",
                |fields| {
                    fields.inputs.pop();
                },
                ExpectedZkError::Groth16ArityMismatch,
            ),
            (
                "extra public input",
                |fields| {
                    fields.inputs.push(fields.inputs[0].clone());
                },
                ExpectedZkError::Groth16ArityMismatch,
            ),
            (
                "short public input",
                |fields| {
                    fields.inputs[0].pop();
                },
                ExpectedZkError::StartsWith("Kaspa txscript error: ZK Integrity: Invalid Fr length:"),
            ),
            (
                "long public input",
                |fields| {
                    fields.inputs[0].push(0);
                },
                ExpectedZkError::StartsWith("Kaspa txscript error: ZK Integrity: Invalid Fr length:"),
            ),
            (
                "out of field public input",
                |fields| {
                    fields.inputs[0] = vec![0xff; 32];
                },
                ExpectedZkError::StartsWith("Kaspa txscript error: ZK Integrity: ARK serialization error:"),
            ),
            (
                "truncated vk",
                |fields| {
                    fields.vk.pop();
                },
                ExpectedZkError::StartsWith("ARK serialization error:"),
            ),
            (
                "trailing vk bytes",
                |fields| {
                    fields.vk.push(0);
                },
                ExpectedZkError::Exact("Groth16 verifying key has trailing bytes"),
            ),
            ("empty gamma_abc_g1", set_empty_gamma_abc, ExpectedZkError::Exact("Groth16 verifying key has empty gamma_abc_g1")),
            ("wrong vk", set_parseable_wrong_vk, ExpectedZkError::Exact("Groth16 verification failed")),
            (
                "truncated proof",
                |fields| {
                    fields.proof.pop();
                },
                ExpectedZkError::StartsWith("ARK serialization error:"),
            ),
            (
                "trailing proof bytes",
                |fields| {
                    fields.proof.push(0);
                },
                ExpectedZkError::Exact("Groth16 proof has trailing bytes"),
            ),
            ("wrong proof", set_parseable_wrong_proof, ExpectedZkError::Exact("Groth16 verification failed")),
            (
                "public input binding",
                |fields| {
                    fields.inputs[0][0] ^= 0x01;
                },
                ExpectedZkError::Exact("Groth16 verification failed"),
            ),
        ];

        let fixture = Groth16Fields::from_fixture();
        for (case, mutate, expected) in cases {
            let mut fields = fixture.clone();
            mutate(&mut fields);
            expect_zk_err(execute_zk_script(&fields.script(), &cache, &reused_values), case, *expected);
        }
    }

    #[test]
    fn verify_groth16_p2sh_vk_in_redeem_script_verifies() {
        let fields = Groth16Fields::from_fixture();
        let (signature_script, redeem_script) = fields.p2sh_scripts();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_p2sh_script(signature_script, &redeem_script, &cache, &reused_values)
            .expect("P2SH Groth16 proof should verify with VK in redeem script");
    }

    #[test]
    fn verify_groth16_p2sh_binding_matrix() {
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        let cases: &[Groth16FailureCase] = &[
            ("vk binding", set_parseable_wrong_vk, ExpectedZkError::Exact("Groth16 verification failed")),
            ("proof binding", set_parseable_wrong_proof, ExpectedZkError::Exact("Groth16 verification failed")),
            (
                "public input binding",
                |fields| {
                    fields.inputs[0][0] ^= 0x01;
                },
                ExpectedZkError::Exact("Groth16 verification failed"),
            ),
        ];

        let fixture = Groth16Fields::from_fixture();
        for (case, mutate, expected) in cases {
            let mut fields = fixture.clone();
            mutate(&mut fields);
            let (signature_script, redeem_script) = fields.p2sh_scripts();
            expect_zk_err(execute_p2sh_script(signature_script, &redeem_script, &cache, &reused_values), case, *expected);
        }
    }

    #[test]
    fn test_r0_succinct_fast() {
        let script = build_stark_script(false);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();

        // Verify ZK static cost estimation formula
        let spk = ScriptPublicKey::from_vec(0, script);
        let estimated = get_zk_script_units_upper_bound::<PopulatedTransaction, SigHashReusedValuesUnsync>(&[], &spk);
        let expected = ZkTag::R0Succinct.cost();
        assert_eq!(estimated, expected);
    }

    #[test]
    fn test_r0_script_builder_groth16() {
        let journal_hash: [u8; 32] =
            hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().try_into().unwrap();
        let image_id: [u8; 32] =
            hex::decode("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap().try_into().unwrap();

        let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
        let rcpt: Groth16Receipt<ReceiptClaim> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();

        let finalized = R0ScriptBuilder::new().commit_to_groth16(image_id).unwrap().finalize_with_proof(rcpt, journal_hash).unwrap();

        execute_p2sh(finalized.sig_script, &finalized.redeem_script).unwrap();
    }

    #[test]
    fn test_r0_script_builder_groth16_fail_invalid_image_id() {
        let journal_hash: [u8; 32] =
            hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().try_into().unwrap();
        let image_id: [u8; 32] =
            hex::decode("70641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap().try_into().unwrap();

        let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
        let rcpt: Groth16Receipt<ReceiptClaim> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();

        let finalized = R0ScriptBuilder::new().commit_to_groth16(image_id).unwrap().finalize_with_proof(rcpt, journal_hash).unwrap();

        match execute_p2sh(finalized.sig_script, &finalized.redeem_script) {
            Ok(_) => panic!("Expected verification to fail due to broken image_id, but it succeeded"),
            Err(TxScriptError::ZkIntegrity(_)) => {}
            Err(e) => panic!("Expected ZkIntegrity error, got different error: {:?}", e),
        }
    }

    #[test]
    fn test_r0_script_builder_groth16_fail_invalid_journal_hash() {
        let journal_hash: [u8; 32] =
            hex::decode("6df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().try_into().unwrap();
        let image_id: [u8; 32] =
            hex::decode("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap().try_into().unwrap();

        let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
        let rcpt: Groth16Receipt<ReceiptClaim> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();

        let finalized = R0ScriptBuilder::new().commit_to_groth16(image_id).unwrap().finalize_with_proof(rcpt, journal_hash).unwrap();

        match execute_p2sh(finalized.sig_script, &finalized.redeem_script) {
            Ok(_) => panic!("Expected verification to fail due to broken journal_hash, but it succeeded"),
            Err(TxScriptError::ZkIntegrity(_)) => {}
            Err(e) => panic!("Expected ZkIntegrity error, got different error: {:?}", e),
        }
    }

    #[test]
    fn test_r0_script_builder_succinct() {
        let succinct_receipt_raw = include_str!("data/zk_builder_tests/succinct.rcpt.hex");
        let image_id_raw = include_str!("data/zk_builder_tests/succinct.image.hex");
        let journal_raw = include_str!("data/zk_builder_tests/succinct.journal.hex");
        let image_id: Digest = hex::decode(image_id_raw).unwrap().try_into().unwrap();
        let journal: Digest = hex::decode(journal_raw).unwrap().try_into().unwrap();
        let rcpt: SuccinctReceipt<ReceiptClaim> = borsh::from_slice(&hex::decode(succinct_receipt_raw).unwrap()).unwrap();

        let finalized = R0ScriptBuilder::new()
            .commit_to_succinct(image_id.as_bytes().try_into().unwrap(), rcpt.control_id.as_bytes().try_into().unwrap(), None)
            .unwrap()
            .finalize_with_proof(rcpt, journal)
            .unwrap();

        execute_p2sh(finalized.sig_script, &finalized.redeem_script).unwrap();
    }

    #[test]
    fn test_r0_succinct_control_id_binding() {
        let script = build_stark_script(true);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        match execute_zk_script(&script, &cache, &reused_values) {
            Ok(_) => panic!("Expected verification to fail due to broken control_id, but it succeeded"),
            Err(e) => match e {
                TxScriptError::ZkIntegrity(e) => {
                    println!("Received expected ZkIntegrity error: {}", e);
                }
                _ => panic!("Expected ZkIntegrity error, got different error: {:?}", e),
            },
        }
    }

    #[test]
    fn verify_r0_succinct_direct_failure_matrix() {
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        let cases: &[R0FailureCase] = &[
            (
                "claim length",
                |fields| {
                    fields.claim.pop();
                },
                ExpectedZkError::Exact("Invalid digest length: 31"),
            ),
            (
                "journal length",
                |fields| {
                    fields.journal.pop();
                },
                ExpectedZkError::Exact("Invalid digest length: 31"),
            ),
            (
                "image id length",
                |fields| {
                    fields.image_id.pop();
                },
                ExpectedZkError::Exact("Invalid digest length: 31"),
            ),
            (
                "control id length",
                |fields| {
                    fields.control_id.pop();
                },
                ExpectedZkError::Exact("Invalid digest length: 31"),
            ),
            (
                "empty hashfn",
                |fields| {
                    fields.hashfn.clear();
                },
                ExpectedZkError::Exact("Invalid hash function encoding length: 0"),
            ),
            (
                "long hashfn",
                |fields| {
                    fields.hashfn.push(0);
                },
                ExpectedZkError::Exact("Invalid hash function encoding length: 2"),
            ),
            (
                "unknown hashfn",
                |fields| {
                    fields.hashfn[0] = 3;
                },
                ExpectedZkError::Exact("Invalid hash function id: 3"),
            ),
            (
                "blake2b hashfn",
                |fields| {
                    fields.hashfn[0] = 0;
                },
                ExpectedZkError::Exact("Unsupported hash function: Blake2b"),
            ),
            (
                "sha256 hashfn",
                |fields| {
                    fields.hashfn[0] = 2;
                },
                ExpectedZkError::Exact("Unsupported hash function: Sha256"),
            ),
            (
                "unaligned seal",
                |fields| {
                    fields.seal.push(0);
                },
                ExpectedZkError::StartsWith("Invalid seal length:"),
            ),
            (
                "control index length",
                |fields| {
                    fields.control_index.pop();
                },
                ExpectedZkError::Exact("Invalid merkle index length: 3"),
            ),
            (
                "control digests length",
                |fields| {
                    fields.control_digests.push(0);
                },
                ExpectedZkError::StartsWith("Invalid digest list length:"),
            ),
            (
                "claim binding",
                |fields| {
                    fields.claim[0] ^= 0x01;
                },
                ExpectedZkError::Exact("R0: journal digest mismatch detected"),
            ),
            (
                "image id binding",
                |fields| {
                    fields.image_id[0] ^= 0x01;
                },
                ExpectedZkError::Exact("Verification failed"),
            ),
            (
                "journal binding",
                |fields| {
                    fields.journal[0] ^= 0x01;
                },
                ExpectedZkError::Exact("Verification failed"),
            ),
            (
                "control id binding",
                |fields| {
                    fields.control_id[0] ^= 0x01;
                },
                ExpectedZkError::StartsWith("R0: control_id mismatch:"),
            ),
            (
                "control index binding",
                |fields| {
                    fields.control_index[0] ^= 0x01;
                },
                ExpectedZkError::StartsWith("R0: control_id mismatch:"),
            ),
            (
                "control proof missing sibling",
                |fields| {
                    truncate_one_digest(&mut fields.control_digests);
                },
                ExpectedZkError::StartsWith("R0: control_id mismatch:"),
            ),
            (
                "control proof extra sibling",
                |fields| {
                    fields.control_digests.extend_from_slice(&[0; risc0_zkp::core::digest::DIGEST_BYTES]);
                },
                ExpectedZkError::Exact("Control inclusion proof length 9 exceeds maximum 8"),
            ),
            (
                "empty seal",
                |fields| {
                    fields.seal.clear();
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
            (
                "short seal",
                |fields| {
                    fields.seal = words_to_le_bytes(&[0; CircuitImpl::OUTPUT_SIZE]);
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
            (
                "first output elem at modulus",
                |fields| {
                    set_seal_word(&mut fields.seal, 0, risc0_zkp::field::baby_bear::P);
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
            (
                "po2 elem above max",
                |fields| {
                    set_seal_word(&mut fields.seal, CircuitImpl::OUTPUT_SIZE, (risc0_zkp::MAX_CYCLES_PO2 + 1) as u32);
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
            (
                "truncated proof after header",
                |fields| {
                    fields.seal.truncate((CircuitImpl::OUTPUT_SIZE + 1) * core::mem::size_of::<u32>());
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
            (
                "trailing seal word",
                |fields| {
                    fields.seal.extend_from_slice(&0u32.to_le_bytes());
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
            (
                "invalid seal merkle digest",
                |fields| {
                    set_seal_word(&mut fields.seal, CircuitImpl::OUTPUT_SIZE + 1, risc0_zkp::field::baby_bear::P);
                },
                ExpectedZkError::Exact("R0: invalid receipt format"),
            ),
        ];

        let fixture = R0Fields::from_fixture();
        for (case, mutate, expected) in cases {
            let mut fields = fixture.clone();
            mutate(&mut fields);
            expect_zk_err(execute_zk_script(&fields.script(), &cache, &reused_values), case, *expected);
        }
    }

    #[test]
    fn verify_r0_succinct_p2sh_split_stack_verifies() {
        let fields = R0Fields::from_fixture();
        let (signature_script, redeem_script) = fields.p2sh_scripts();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_p2sh_script(signature_script, &redeem_script, &cache, &reused_values).expect("split P2SH R0 proof should verify");
    }

    #[test]
    fn verify_r0_succinct_p2sh_binding_matrix() {
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        let cases: &[R0FailureCase] = &[
            (
                "claim binding",
                |fields| {
                    fields.claim[0] ^= 0x01;
                },
                ExpectedZkError::Exact("R0: journal digest mismatch detected"),
            ),
            (
                "journal binding",
                |fields| {
                    fields.journal[0] ^= 0x01;
                },
                ExpectedZkError::Exact("Verification failed"),
            ),
            (
                "image id binding",
                |fields| {
                    fields.image_id[0] ^= 0x01;
                },
                ExpectedZkError::Exact("Verification failed"),
            ),
            (
                "control id binding",
                |fields| {
                    fields.control_id[0] ^= 0x01;
                },
                ExpectedZkError::StartsWith("R0: control_id mismatch:"),
            ),
        ];

        let fixture = R0Fields::from_fixture();
        for (case, mutate, expected) in cases {
            let mut fields = fixture.clone();
            mutate(&mut fields);
            let (signature_script, redeem_script) = fields.p2sh_scripts();
            expect_zk_err(execute_p2sh_script(signature_script, &redeem_script, &cache, &reused_values), case, *expected);
        }
    }

    #[test]
    fn test_r0_succinct_not_field_elem() {
        let (_, seal, _, _, _, _, _, _) = load_stark_fields();
        let seal_words = seal.as_chunks().0.iter().copied().map(u32::from_le_bytes).collect::<Vec<_>>();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        let cases = [
            ("first output elem at modulus", 0, risc0_zkp::field::baby_bear::P),
            ("last output elem at modulus", CircuitImpl::OUTPUT_SIZE - 1, risc0_zkp::field::baby_bear::P),
            ("po2 elem above max", CircuitImpl::OUTPUT_SIZE, (risc0_zkp::MAX_CYCLES_PO2 + 1) as u32),
        ];

        for (case, i, invalid_word) in cases {
            let mut seal_words = seal_words.clone();
            seal_words[i] = invalid_word;
            let seal = words_to_le_bytes(&seal_words);
            let script = r0_script_with_seal(&seal);

            expect_r0_receipt_format_err(execute_zk_script(&script, &cache, &reused_values), case);
        }
    }

    #[test]
    fn verify_r0_succinct_short_seal_rejected() {
        let seal = words_to_le_bytes(&[0; CircuitImpl::OUTPUT_SIZE]);
        let script = r0_script_with_seal(&seal);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_zk_script(&script, &cache, &reused_values).expect_err("short seal should fail");
    }

    #[test]
    fn verify_r0_succinct_invalid_po2_rejected() {
        let mut seal_words = vec![0; CircuitImpl::OUTPUT_SIZE + 1];
        seal_words[CircuitImpl::OUTPUT_SIZE] = (risc0_zkp::MAX_CYCLES_PO2 + 1) as u32;
        let seal = words_to_le_bytes(&seal_words);
        let script = r0_script_with_seal(&seal);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_zk_script(&script, &cache, &reused_values).expect_err("invalid po2 should fail");
    }

    #[test]
    fn verify_r0_succinct_truncated_proof_after_header_rejected() {
        let (_, seal, _, _, _, _, _, _) = load_stark_fields();
        let header_len = (CircuitImpl::OUTPUT_SIZE + 1) * core::mem::size_of::<u32>();
        let script = r0_script_with_seal(&seal[..header_len]);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_zk_script(&script, &cache, &reused_values).expect_err("truncated proof should fail");
    }

    #[test]
    fn verify_r0_succinct_trailing_word_rejected() {
        let (_, mut seal, _, _, _, _, _, _) = load_stark_fields();
        seal.extend_from_slice(&0u32.to_le_bytes());
        let script = r0_script_with_seal(&seal);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_zk_script(&script, &cache, &reused_values).expect_err("trailing proof data should fail");
    }

    #[test]
    fn verify_r0_succinct_invalid_control_proof_digest_rejected() {
        let (_, _, _, _, _, mut control_digests, _, _) = load_stark_fields();
        control_digests[..core::mem::size_of::<u32>()].copy_from_slice(&risc0_zkp::field::baby_bear::P.to_le_bytes());
        let script = r0_script_with_control_digests(&control_digests);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        execute_zk_script(&script, &cache, &reused_values).expect_err("invalid Poseidon2 control proof digest should fail");
    }

    #[test]
    fn verify_r0_succinct_invalid_control_id_digest_rejected() {
        let (mut control_id, _, _, _, _, _, _, _) = load_stark_fields();
        control_id[..core::mem::size_of::<u32>()].copy_from_slice(&risc0_zkp::field::baby_bear::P.to_le_bytes());
        let script = r0_script_with_control_id(&control_id);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        match execute_zk_script(&script, &cache, &reused_values) {
            Err(TxScriptError::ZkIntegrity(e)) if e.starts_with("R0: control_id mismatch:") => {}
            Err(e) => panic!("expected R0 control_id mismatch, got {e:?}"),
            Ok(_) => panic!("invalid Poseidon2 control id digest should fail"),
        }
    }

    #[test]
    fn verify_r0_succinct_invalid_seal_merkle_digest_rejected() {
        let (_, seal, _, _, _, _, _, _) = load_stark_fields();
        let mut seal_words = seal.as_chunks().0.iter().copied().map(u32::from_le_bytes).collect::<Vec<_>>();
        seal_words[CircuitImpl::OUTPUT_SIZE + 1] = risc0_zkp::field::baby_bear::P;
        let seal = words_to_le_bytes(&seal_words);
        let script = r0_script_with_seal(&seal);
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        expect_r0_receipt_format_err(execute_zk_script(&script, &cache, &reused_values), "invalid seal Merkle digest");
    }

    #[test]
    fn verify_r0_succinct_long_control_proof_rejected() {
        let mut fields = R0Fields::from_fixture();
        fields.control_digests.extend_from_slice(&[0; risc0_zkp::core::digest::DIGEST_BYTES]);
        let script = fields.script();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        expect_zk_err(
            execute_zk_script(&script, &cache, &reused_values),
            "long control proof",
            ExpectedZkError::Exact("Control inclusion proof length 9 exceeds maximum 8"),
        );
    }
}
