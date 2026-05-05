pub mod helpers;

#[cfg(test)]
mod fast_zk_tests {
    use super::helpers::{build_groth_script, build_stark_script, execute_zk_script};
    use crate::{
        EngineCtx, EngineFlags, SigCacheKey, TxScriptEngine, caches::Cache, get_zk_script_units_upper_bound, hex, pay_to_script_hash_script, zk_precompiles::{risc0::zk_to_script::R0ScriptBuilder, tags::ZkTag}
    };
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync, subnets::SubnetworkId, tx::{PopulatedTransaction, ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput}
    };
    use kaspa_consensus_core::tx::{
   UtxoEntry,
};
    use kaspa_hashes::Hash;
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

   #[test]  
fn test_r0_script_builder_groth16() {  
    let journal_hash: [u8; 32] =  
        hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().try_into().unwrap();  
    let image_id: [u8; 32] =  
        hex::decode("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap().try_into().unwrap();  
  
    let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");  
    let rcpt: Groth16Receipt<ReceiptClaim> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();  
  
    // Build the Groth16 script  
    let zk_script_builder = R0ScriptBuilder::new();  
    let zk_script_builder = zk_script_builder.commit_to_groth16(image_id).unwrap();  
    let commit_script = zk_script_builder.finalize_with_proof(rcpt, journal_hash).unwrap();  
  
    // Create P2SH script public key  
    let spk = pay_to_script_hash_script(&commit_script);  
  
  
    // Create dummy transaction  
    let dummy_outpoint = TransactionOutpoint::new(Hash::from_u64_word(0), 0);  
    let input = TransactionInput::new(dummy_outpoint, commit_script, 0, 0);  
    let output = TransactionOutput::new(1_000_000, spk.clone());  
    let mut tx = Transaction::new(0, vec![input], vec![output], 0, SubnetworkId::default(), 0, vec![]);  
    tx.finalize();  
  
    // Create UTXO entry with the P2SH script  
    let utxo_entry = UtxoEntry::new(1_000_000, spk, 0, false,None);  
  
    // Execute through full P2SH validation  
    let sig_cache:Cache<SigCacheKey, bool> = Cache::new(10_000);  
    let reused_values = SigHashReusedValuesUnsync::new();  
    let flags = EngineFlags { covenants_enabled: true, ..Default::default() };  
  
    let populated = PopulatedTransaction::new(&tx, vec![utxo_entry]);  
    let mut vm = TxScriptEngine::from_transaction_input(  
        &populated,  
        &tx.inputs[0],  
        0,  
        &populated.entries[0],  
        EngineCtx::new(&sig_cache).with_reused(&reused_values),  
        flags,  
    );  
      
    vm.execute().unwrap();  
}
    #[test]
    fn test_r0_script_builder_groth16_fail_invalid_image_id() {
        let journal_hash: [u8; 32] =
            hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().try_into().unwrap();
        let image_id: [u8; 32] =
            hex::decode("70641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap().try_into().unwrap();

        let groth_receipt_raw = include_str!("data/zk_builder_tests/groth.rcpt.hex");
        let rcpt: Groth16Receipt<ReceiptClaim> = borsh::from_slice(&hex::decode(groth_receipt_raw).unwrap()).unwrap();

        let zk_script_builder = R0ScriptBuilder::new();
        let zk_script_builder = zk_script_builder.commit_to_groth16(image_id).unwrap();
        let script = zk_script_builder.finalize_with_proof(rcpt, journal_hash).unwrap();

        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        match execute_zk_script(&script, &cache, &reused_values) {
            Ok(_) => panic!("Expected verification to fail due to broken image_id, but it succeeded"),
            Err(e) => match e {
                TxScriptError::ZkIntegrity(_) => {}
                _ => panic!("Expected ZkIntegrity error, got different error: {:?}", e),
            },
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

        let zk_script_builder = R0ScriptBuilder::new();
        let zk_script_builder = zk_script_builder.commit_to_groth16(image_id).unwrap();
        let script = zk_script_builder.finalize_with_proof(rcpt, journal_hash).unwrap();

        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        match execute_zk_script(&script, &cache, &reused_values) {
            Ok(_) => panic!("Expected verification to fail due to broken journal_hash, but it succeeded"),
            Err(e) => match e {
                TxScriptError::ZkIntegrity(_) => {}
                _ => panic!("Expected ZkIntegrity error, got different error: {:?}", e),
            },
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
        let zk_script_builder = R0ScriptBuilder::new();
        let zk_script_builder = zk_script_builder
            .commit_to_succinct(image_id.as_bytes().try_into().unwrap(), rcpt.control_id.as_bytes().try_into().unwrap(), None)
            .unwrap();
        let script = zk_script_builder.finalize_with_proof(rcpt, journal.as_bytes().try_into().unwrap()).unwrap();
        let cache = Cache::new(0);
        let reused_values = SigHashReusedValuesUnsync::new();
        execute_zk_script(&script, &cache, &reused_values).unwrap();
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


}
