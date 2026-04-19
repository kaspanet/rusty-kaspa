pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{build_groth_script, build_stark_script, build_zk_script, execute_zk_script, load_stark_fields};
    use crate::{caches::Cache, get_zk_script_units_upper_bound, zk_precompiles::tags::ZkTag};
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };
    use kaspa_txscript_errors::TxScriptError;

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
        let (control_id, seal, claim, hashfn, control_index, control_digests, journal, image_id) = load_stark_fields();
        let seal_words = seal.as_chunks().0.iter().copied().map(u32::from_le_bytes).collect::<Vec<_>>();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        for i in 0..seal_words.len() {
            let mut seal_words = seal_words.clone();
            // we add modular group order to the seal words to make sure they are not field elements but they are still valid u32 values
            let Some(v) = seal_words[i].checked_add(risc0_zkp::field::baby_bear::P) else {
                continue;
            };
            seal_words[i] = v;
            let stark_tag = ZkTag::R0Succinct as u8;
            let seal = bytemuck::cast_slice(seal_words.as_slice());
            let script = build_zk_script(&[
                &claim,
                &control_index,
                &control_digests,
                seal,
                &journal,
                &image_id,
                &control_id,
                &hashfn,
                &[stark_tag],
            ])
            .unwrap();
            // Verify execution
            execute_zk_script(&script, &cache, &reused_values).expect_err("should fail");
        }
    }
}
