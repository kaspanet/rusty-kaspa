pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{
        build_groth_script, build_groth_script_from_fields, build_stark_script, build_zk_script, execute_zk_script, load_groth_fields,
        load_stark_fields,
    };
    use crate::{
        caches::Cache,
        get_zk_script_units_upper_bound,
        zk_precompiles::{groth16::Groth16Error, tags::ZkTag},
    };
    use ark_groth16::VerifyingKey;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };
    use kaspa_txscript_errors::TxScriptError;
    use risc0_circuit_recursion::CircuitImpl;
    use risc0_zkp::adapter::CircuitInfo;

    fn r0_script_with_seal(seal: &[u8]) -> Vec<u8> {
        let (control_id, _, claim, hashfn, control_index, control_digests, journal, image_id) = load_stark_fields();
        let stark_tag = ZkTag::R0Succinct as u8;
        build_zk_script(&[&claim, &control_index, &control_digests, seal, &journal, &image_id, &control_id, &hashfn, &[stark_tag]])
            .unwrap()
    }

    fn r0_script_with_control_digests(control_digests: &[u8]) -> Vec<u8> {
        let (control_id, seal, claim, hashfn, control_index, _, journal, image_id) = load_stark_fields();
        let stark_tag = ZkTag::R0Succinct as u8;
        build_zk_script(&[&claim, &control_index, control_digests, &seal, &journal, &image_id, &control_id, &hashfn, &[stark_tag]])
            .unwrap()
    }

    fn r0_script_with_control_id(control_id: &[u8]) -> Vec<u8> {
        let (_, seal, claim, hashfn, control_index, control_digests, journal, image_id) = load_stark_fields();
        let stark_tag = ZkTag::R0Succinct as u8;
        build_zk_script(&[&claim, &control_index, &control_digests, &seal, &journal, &image_id, control_id, &hashfn, &[stark_tag]])
            .unwrap()
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

    fn expect_groth16_arity_mismatch(result: Result<(), TxScriptError>, case: &str) {
        let expected = Groth16Error::ArkR1CS(ark_relations::gr1cs::SynthesisError::ArityMismatch).to_string();
        match result {
            Err(TxScriptError::ZkIntegrity(e)) if e == expected => {}
            Err(e) => panic!("{case}: expected Groth16 arity mismatch, got {e:?}"),
            Ok(_) => panic!("{case}: expected Groth16 arity mismatch, got success"),
        }
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
}
