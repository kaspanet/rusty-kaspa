pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{build_groth_script, build_stark_script, execute_zk_script};
    use crate::{
        caches::Cache, get_zk_script_units_upper_bound, hex,
        zk_precompiles::{risc0::zk_to_script::R0ScriptBuilder, tags::ZkTag},
    };
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };
    use risc0_zkvm::{Digest, Groth16Receipt, MaybePruned, ReceiptClaim, SuccinctReceipt};

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
        let succinct_receipt_raw = include_str!("data/zk_builder_tests/succinct.rcpt.hex");
        let image_id_raw = include_str!("data/zk_builder_tests/succinct.image.hex");
        let journal_raw = include_str!("data/zk_builder_tests/succinct.journal.hex");
        let image_id: Digest = hex::decode(image_id_raw).unwrap().try_into().unwrap();
        let journal: Digest = hex::decode(journal_raw).unwrap().try_into().unwrap();
        let rcpt: SuccinctReceipt<MaybePruned<ReceiptClaim>> = borsh::from_slice(&hex::decode(succinct_receipt_raw).unwrap()).unwrap();
        let mut script_builder = R0ScriptBuilder::from_succinct(&rcpt, journal, image_id).unwrap();
        let script = script_builder.drain();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();
    }

    #[test]
    fn test_groth16_rcpt_to_kaspa_script() {
        let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
        let rcpt: Groth16Receipt<MaybePruned<ReceiptClaim>> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();
        println!("Groth16 Receipt: {:?}", rcpt.seal);

        let mut script_builder = R0ScriptBuilder::from_groth(&rcpt).unwrap();
        let script = script_builder.drain();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();
        
    }
}
