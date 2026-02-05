pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{build_groth_script, build_stark_script, execute_zk_script};
    use crate::{caches::Cache, get_sig_op_count_upper_bound, zk_precompiles::tags::ZkTag};
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };

    #[test]
    fn test_groth16_fast() {
        let script = build_groth_script();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();

        // Verify static sigop count for Groth16
        let spk = ScriptPublicKey::from_vec(0, script);
        let sigops = get_sig_op_count_upper_bound::<PopulatedTransaction, SigHashReusedValuesUnsync>(&[], &spk);
        assert_eq!(sigops, ZkTag::Groth16.sigop_cost() as u64);
    }

    #[test]
    fn test_r0_succinct_fast() {
        let script = build_stark_script();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();

        // Verify static sigop count for R0Succinct
        let spk = ScriptPublicKey::from_vec(0, script);
        let sigops = get_sig_op_count_upper_bound::<PopulatedTransaction, SigHashReusedValuesUnsync>(&[], &spk);
        assert_eq!(sigops, ZkTag::R0Succinct.sigop_cost() as u64);
    }
}
