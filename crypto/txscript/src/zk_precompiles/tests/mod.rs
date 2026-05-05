pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{build_groth_script, build_stark_script, execute_zk_script};
    use crate::{
        caches::Cache,
        get_zk_script_units_upper_bound, hex,
        opcodes::codes::{OpCat, OpDrop, OpDup, OpEqual, OpEqualVerify, OpPick, OpRot, OpSHA256, OpSwap},
        script_builder::ScriptBuilder,
        zk_precompiles::{
            risc0::zk_to_script::{R0ScriptBuilder, groth16::{append_locking_groth16, append_spending_groth16},},
            tags::ZkTag,
        },
    };
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{PopulatedTransaction, ScriptPublicKey},
    };
    use kaspa_txscript_errors::TxScriptError;
    use risc0_zkvm::{Digest, Groth16Receipt, ReceiptClaim, SuccinctReceipt};

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
    fn build_receipt_claim_script(journal_hash: &[u8; 32], image_id: &[u8; 32]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut builder = ScriptBuilder::new();

        let post_digest = hex::decode("a3acc27117418996340b84e5a90f3ef4c49d22c79e44aad822ec9c313e1eb8e2")?;
        let expected = hex::decode("a95ac0b37bfedcd8136e6c1143086bf5d223ffcb21c6ffcb7c8f60392ca49dde")?;

        // ===== Output digest =====
        // SHA256( SHA256("risc0.Output") || journal_hash || ZERO_assumptions || u16_le(2) )
        //
        // journal_hash is already a Pruned Digest in ReceiptClaim::ok — do NOT re-hash it.
        builder.add_data(b"risc0.Output")?;
        builder.add_op(OpSHA256)?; // [tag_hash]
        builder.add_data(journal_hash)?; // [tag_hash, journal]
        builder.add_op(OpCat)?; // [tag_hash || journal]
        builder.add_data(&[0u8; 32])?; // ZERO assumptions
        builder.add_op(OpCat)?;
        builder.add_data(&2u16.to_le_bytes())?; // down count = 2
        builder.add_op(OpCat)?;
        builder.add_op(OpSHA256)?; // [output_digest]

        // ===== ReceiptClaim digest =====
        // SHA256( SHA256("risc0.ReceiptClaim") || ZERO_input || image_id || post_digest
        //       || output_digest || u32_le(0) || u32_le(0) || u16_le(4) )
        //
        // output_digest is on the stack. Build the preamble (tag||input||pre||post)
        // beside it, then OpSwap + OpCat to splice output_digest into position,
        // then append the tail.
        builder.add_data(b"risc0.ReceiptClaim")?;
        builder.add_op(OpSHA256)?; // [output_digest, tag_hash]
        builder.add_data(&[0u8; 32])?; // ZERO input
        builder.add_op(OpCat)?; // [output_digest, tag_hash || ZERO]
        builder.add_data(image_id)?;
        builder.add_op(OpCat)?; // [output_digest, ... || image_id]
        builder.add_data(&post_digest)?;
        builder.add_op(OpCat)?; // [output_digest, preamble]

        builder.add_op(OpSwap)?; // [preamble, output_digest]
        builder.add_op(OpCat)?; // [preamble || output_digest]

        builder.add_data(&0u32.to_le_bytes())?; // sys_exit
        builder.add_op(OpCat)?;
        builder.add_data(&0u32.to_le_bytes())?; // user_exit
        builder.add_op(OpCat)?;
        builder.add_data(&4u16.to_le_bytes())?; // down count = 4
        builder.add_op(OpCat)?;
        builder.add_op(OpSHA256)?; // [receipt_claim_digest]

        // ===== Verify =====
        // OpEqual (not OpEqualVerify) so we leave [1] on the stack for the
        // executor's final truthy-stack check. Mismatch leaves [] and the script
        // ends as falsy.
        builder.add_data(&expected)?;
        builder.add_op(OpEqual)?;

        Ok(builder.drain())
    }
    #[test]
    fn test_receipt_claim_script() {
        let journal_hash = hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap();
        let image_id = hex::decode("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap();

        let script = build_receipt_claim_script(&journal_hash.try_into().unwrap(), &image_id.try_into().unwrap()).unwrap();

        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();
    }

   #[test]
fn test_receipt_claim_script2() {
    let journal_hash: [u8; 32] = hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456")
        .unwrap()
        .try_into()
        .unwrap();
    let image_id: [u8; 32] = hex::decode("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0")
        .unwrap()
        .try_into()
        .unwrap();

    let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
    let rcpt: Groth16Receipt<ReceiptClaim> =
        borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();

    // Spending script (witness): pushes proof, then journal_hash.
    let mut sig_sb = ScriptBuilder::new();
    append_spending_groth16(&mut sig_sb, &rcpt, &journal_hash).unwrap();
    let sig_script = sig_sb.drain();

    // Locking script (SPK): everything fixed at UTXO creation.
    let mut spk_sb = ScriptBuilder::new();
    append_locking_groth16(&mut spk_sb, &image_id).unwrap();
    let spk_script = spk_sb.drain();

    // execute_zk_script runs a single byte stream; concatenate sig + spk.
    let mut full = sig_script;
    full.extend_from_slice(&spk_script);

    let cache = Cache::new(0);
    let reused_values = SigHashReusedValuesUnsync::new();
    execute_zk_script(&full, &cache, &reused_values).unwrap();
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

    /*
    #[test]
    fn test_r0_succinct_rcpt_to_kaspa_script() {
        let succinct_receipt_raw = include_str!("data/zk_builder_tests/succinct.rcpt.hex");
        let image_id_raw = include_str!("data/zk_builder_tests/succinct.image.hex");
        let journal_raw = include_str!("data/zk_builder_tests/succinct.journal.hex");
        let image_id: Digest = hex::decode(image_id_raw).unwrap().try_into().unwrap();
        let journal: Digest = hex::decode(journal_raw).unwrap().try_into().unwrap();
        let rcpt: SuccinctReceipt<ReceiptClaim> = borsh::from_slice(&hex::decode(succinct_receipt_raw).unwrap()).unwrap();
        let zk_script_builder = R0ScriptBuilder::new();
        let zk_script_builder = zk_script_builder.initialize().unwrap();
        let zk_script_builder = zk_script_builder.bind_to_stark(image_id).unwrap();
        let zk_script_builder = zk_script_builder.add_proof_data(rcpt, journal).unwrap();
        let mut zk_script_builder = zk_script_builder.finalize();
        let script = zk_script_builder.drain();

        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();
    } */
    /*


    #[test]
    fn test_groth16_rcpt_to_kaspa_script() {
        let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
        let rcpt: Groth16Receipt<ReceiptClaim> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();
        let zk_script_builder = R0ScriptBuilder::new();
        let zk_script_builder = zk_script_builder.initialize().unwrap();
        let zk_script_builder = zk_script_builder.bind_to_groth16().unwrap();
        let zk_script_builder = zk_script_builder.add_proof_data(rcpt).unwrap();
        let mut zk_script_builder = zk_script_builder.finalize();
        let script = zk_script_builder.drain();

        let mut script_builder = R0ScriptBuilder::from_groth(&rcpt).unwrap();
        let script = script_builder.drain();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();

        // Verify execution
        execute_zk_script(&script, &cache, &reused_values).unwrap();
    }*/
}
