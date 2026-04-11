pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{build_groth_script, build_stark_script, execute_zk_script};
    use crate::{caches::Cache, get_sig_op_count_upper_bound, hex, zk_precompiles::{tags::ZkTag, zk_to_script::ZkScriptBuilder}};
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };
    use risc0_zkvm::{Digest, SuccinctReceipt};

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
        let script = build_stark_script();
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
    fn test_r0_succinct_rcpt_to_kaspa_script() {
        //let script_builder=ZkScriptBuilder::from_succinct(receipt, journal, image_id)
        let succinct_receipt_raw = include_str!("data/zk_builder_tests/succinct.rcpt.hex");
        let rcpt:SuccinctReceipt<Digest>=borsh::from_slice(&hex::decode(succinct_receipt_raw).unwrap()).unwrap();
        println!("Parsed receipt: {:?}", rcpt.seal);
    }
}
