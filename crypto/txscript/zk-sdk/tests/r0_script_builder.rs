use ark_bn254::{Bn254, G1Affine};
use ark_groth16::Proof;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use kaspa_consensus_core::{
    hashing::sighash::SigHashReusedValuesUnsync,
    subnets::SubnetworkId,
    tx::{PopulatedTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_hashes::Hash;
use kaspa_txscript::{EngineCtx, EngineFlags, SigCacheKey, TxScriptEngine, caches::Cache, hex, pay_to_script_hash_script};
use kaspa_txscript_errors::TxScriptError;
use kaspa_txscript_zk_sdk::{ZkScriptBuilder, prepare_r0_groth16_proof};
use risc0_zkvm::{Digest, Groth16Receipt, ReceiptClaim, SuccinctReceipt};

fn zk_test_flags() -> EngineFlags {
    EngineFlags { covenants_enabled: true, ..Default::default() }
}

fn execute_p2sh(sig_script: Vec<u8>, redeem_script: &[u8]) -> Result<(), TxScriptError> {
    let spk = pay_to_script_hash_script(redeem_script);

    let dummy_outpoint = TransactionOutpoint::new(Hash::from_u64_word(0), 0);
    let input = TransactionInput::new(dummy_outpoint, sig_script, 0, 0);
    let output = TransactionOutput::new(1_000_000, spk.clone());
    let mut tx = Transaction::new(0, vec![input], vec![output], 0, SubnetworkId::default(), 0, vec![]);
    tx.finalize();

    let utxo_entry = UtxoEntry::new(1_000_000, spk, 0, false, None);
    let sig_cache: Cache<SigCacheKey, bool> = Cache::new(10_000);
    let reused_values = SigHashReusedValuesUnsync::new();
    let populated = PopulatedTransaction::new(&tx, vec![utxo_entry]);
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated,
        &tx.inputs[0],
        0,
        &populated.entries[0],
        EngineCtx::new(&sig_cache).with_reused(&reused_values),
        zk_test_flags(),
    );
    vm.execute()
}

fn load_groth_fixture() -> ([u8; 32], [u8; 32], Groth16Receipt<ReceiptClaim>) {
    let journal_hash: [u8; 32] =
        hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456").unwrap().try_into().unwrap();
    let image_id: [u8; 32] =
        hex::decode("75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0").unwrap().try_into().unwrap();
    let receipt: Groth16Receipt<ReceiptClaim> =
        borsh::from_slice(&hex::decode(include_str!("data/zk_builder_tests/groth.rcpt.hex")).unwrap()).unwrap();
    (journal_hash, image_id, receipt)
}

fn load_succinct_fixture() -> (Digest, Digest, SuccinctReceipt<ReceiptClaim>) {
    let image_id: Digest = hex::decode(include_str!("data/zk_builder_tests/succinct.image.hex")).unwrap().try_into().unwrap();
    let journal: Digest = hex::decode(include_str!("data/zk_builder_tests/succinct.journal.hex")).unwrap().try_into().unwrap();
    let receipt: SuccinctReceipt<ReceiptClaim> =
        borsh::from_slice(&hex::decode(include_str!("data/zk_builder_tests/succinct.rcpt.hex")).unwrap()).unwrap();
    (image_id, journal, receipt)
}

#[test]
fn r0_script_builder_groth16_verifies() {
    let (journal_hash, image_id, receipt) = load_groth_fixture();
    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_groth16(image_id)
        .unwrap()
        .finalize_with_proof(receipt, journal_hash)
        .unwrap();
    execute_p2sh(finalized.sig_script, &finalized.redeem_script).unwrap();
}

#[test]
fn r0_script_builder_groth16_binds_image_id() {
    let (journal_hash, mut image_id, receipt) = load_groth_fixture();
    image_id[0] = 0x70; // corrupt the image id
    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_groth16(image_id)
        .unwrap()
        .finalize_with_proof(receipt, journal_hash)
        .unwrap();
    assert!(matches!(execute_p2sh(finalized.sig_script, &finalized.redeem_script), Err(TxScriptError::ZkIntegrity(_))));
}

#[test]
fn r0_script_builder_groth16_binds_journal_hash() {
    let (mut journal_hash, image_id, receipt) = load_groth_fixture();
    journal_hash[0] = 0x6d; // corrupt the journal hash
    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_groth16(image_id)
        .unwrap()
        .finalize_with_proof(receipt, journal_hash)
        .unwrap();
    assert!(matches!(execute_p2sh(finalized.sig_script, &finalized.redeem_script), Err(TxScriptError::ZkIntegrity(_))));
}

#[test]
fn r0_script_builder_succinct_verifies() {
    let (image_id, journal, receipt) = load_succinct_fixture();
    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_succinct(image_id.as_bytes().try_into().unwrap(), receipt.control_id.as_bytes().try_into().unwrap(), None)
        .unwrap()
        .finalize_with_proof(receipt, journal)
        .unwrap();
    execute_p2sh(finalized.sig_script, &finalized.redeem_script).unwrap();
}

#[test]
fn r0_script_builder_groth16_fixed_journal_verifies() {
    let (journal_hash, image_id, receipt) = load_groth_fixture();
    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_groth16_with_fixed_journal(image_id, journal_hash)
        .unwrap()
        .finalize_with_proof(receipt)
        .unwrap();
    execute_p2sh(finalized.sig_script, &finalized.redeem_script).unwrap();
}

#[test]
fn r0_script_builder_succinct_fixed_journal_verifies() {
    let (image_id, journal, receipt) = load_succinct_fixture();
    let image_id_bytes: [u8; 32] = image_id.as_bytes().try_into().unwrap();
    let control_id: [u8; 32] = receipt.control_id.as_bytes().try_into().unwrap();
    let journal_bytes: [u8; 32] = journal.as_bytes().try_into().unwrap();
    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_succinct_with_fixed_journal(image_id_bytes, control_id, None, journal_bytes)
        .unwrap()
        .finalize_with_proof(receipt)
        .unwrap();
    execute_p2sh(finalized.sig_script, &finalized.redeem_script).unwrap();
}

#[test]
fn r0_script_builder_groth16_rejects_tampered_proof() {
    let flags = zk_test_flags();
    let (journal_hash, image_id, receipt) = load_groth_fixture();

    // Start from the valid compressed proof, then replace one group element with
    // a different (still on-curve) point: structurally valid, cryptographically wrong.
    let proof_bytes = prepare_r0_groth16_proof(&receipt).unwrap();
    let mut proof = Proof::<Bn254>::deserialize_compressed(proof_bytes.as_slice()).unwrap();
    proof.a = G1Affine::default();
    let mut tampered_proof = Vec::new();
    proof.serialize_compressed(&mut tampered_proof).unwrap();
    assert_ne!(tampered_proof, proof_bytes, "tamper must actually change the proof");

    // Verifier (redeem) is built normally; the signature script carries the tampered proof.
    let mut redeem_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    redeem_builder.append_r0_groth16_verifier(image_id).unwrap();
    let redeem_script = redeem_builder.drain();

    let mut sig_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    sig_builder.add_data(&journal_hash).unwrap();
    sig_builder.add_data(&tampered_proof).unwrap();
    sig_builder.add_data(&redeem_script).unwrap();
    let sig_script = sig_builder.drain();

    assert!(matches!(execute_p2sh(sig_script, &redeem_script), Err(TxScriptError::ZkIntegrity(_))));
}

#[test]
fn r0_script_builder_succinct_rejects_tampered_proof() {
    let (image_id, journal, mut receipt) = load_succinct_fixture();
    receipt.seal[0] ^= 1; // flip a single bit in the STARK seal

    let finalized = ZkScriptBuilder::new_r0_with_flags(zk_test_flags())
        .commit_to_succinct(image_id.as_bytes().try_into().unwrap(), receipt.control_id.as_bytes().try_into().unwrap(), None)
        .unwrap()
        .finalize_with_proof(receipt, journal)
        .unwrap();
    assert!(matches!(execute_p2sh(finalized.sig_script, &finalized.redeem_script), Err(TxScriptError::ZkIntegrity(_))));
}

#[test]
fn fragments_groth16_roundtrip_matches_facade() {
    let flags = zk_test_flags();
    let (journal_hash, image_id, receipt) = load_groth_fixture();

    // Redeem (verifier) script via the fragment method.
    let mut redeem_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    redeem_builder.append_r0_groth16_verifier(image_id).unwrap();
    let redeem_script = redeem_builder.drain();

    // Signature script: caller pushes journal_hash, then the proof witness, then redeem.
    let mut sig_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    sig_builder.add_data(&journal_hash).unwrap();
    sig_builder.push_r0_groth16_proof(receipt).unwrap();
    sig_builder.add_data(&redeem_script).unwrap();
    let sig_script = sig_builder.drain();

    // Byte-identity against the high-level facade.
    let (_, _, facade_receipt) = load_groth_fixture();
    let facade = ZkScriptBuilder::new_r0_with_flags(flags)
        .commit_to_groth16(image_id)
        .unwrap()
        .finalize_with_proof(facade_receipt, journal_hash)
        .unwrap();
    assert_eq!(redeem_script, facade.redeem_script, "fragment redeem must match commit_to_groth16 bytes");
    assert_eq!(sig_script, facade.sig_script, "fragment sig must match finalize_with_proof bytes");

    execute_p2sh(sig_script, &redeem_script).unwrap();
}

#[test]
fn fragments_groth16_fixed_journal_covenant() {
    let flags = zk_test_flags();
    let (journal_hash, image_id, receipt) = load_groth_fixture();

    let mut redeem_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    redeem_builder.append_r0_groth16_verifier_with_fixed_journal(image_id, journal_hash).unwrap();
    let redeem_script = redeem_builder.drain();

    let mut sig_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    sig_builder.push_r0_groth16_proof(receipt).unwrap();
    sig_builder.add_data(&redeem_script).unwrap();
    let sig_script = sig_builder.drain();

    execute_p2sh(sig_script, &redeem_script).unwrap();
}

#[test]
fn fragments_groth16_fixed_journal_wrong_journal_rejected() {
    let flags = zk_test_flags();
    let (mut journal_hash, image_id, receipt) = load_groth_fixture();
    journal_hash[0] ^= 0xFF;

    let mut redeem_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    redeem_builder.append_r0_groth16_verifier_with_fixed_journal(image_id, journal_hash).unwrap();
    let redeem_script = redeem_builder.drain();

    let mut sig_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    sig_builder.push_r0_groth16_proof(receipt).unwrap();
    sig_builder.add_data(&redeem_script).unwrap();
    let sig_script = sig_builder.drain();

    assert!(matches!(execute_p2sh(sig_script, &redeem_script), Err(TxScriptError::ZkIntegrity(_))));
}

#[test]
fn fragments_succinct_roundtrip_matches_facade() {
    let flags = zk_test_flags();
    let (image_id, journal, receipt) = load_succinct_fixture();
    let control_id: [u8; 32] = receipt.control_id.as_bytes().try_into().unwrap();
    let image_id_bytes: [u8; 32] = image_id.as_bytes().try_into().unwrap();

    let mut redeem_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    redeem_builder.append_r0_succinct_verifier(image_id_bytes, control_id, None).unwrap();
    let redeem_script = redeem_builder.drain();

    // Signature script: receipt witness items, then journal on top, then redeem.
    let mut sig_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    sig_builder.push_r0_succinct_witness(receipt).unwrap();
    sig_builder.add_data(journal.as_bytes()).unwrap();
    sig_builder.add_data(&redeem_script).unwrap();
    let sig_script = sig_builder.drain();

    let (_, _, facade_receipt) = load_succinct_fixture();
    let facade = ZkScriptBuilder::new_r0_with_flags(flags)
        .commit_to_succinct(image_id_bytes, control_id, None)
        .unwrap()
        .finalize_with_proof(facade_receipt, journal)
        .unwrap();
    assert_eq!(redeem_script, facade.redeem_script, "fragment redeem must match commit_to_succinct bytes");
    assert_eq!(sig_script, facade.sig_script, "fragment sig must match finalize_with_proof bytes");

    execute_p2sh(sig_script, &redeem_script).unwrap();
}

#[test]
fn fragments_succinct_fixed_journal_covenant() {
    let flags = zk_test_flags();
    let (image_id, journal, receipt) = load_succinct_fixture();
    let control_id: [u8; 32] = receipt.control_id.as_bytes().try_into().unwrap();
    let image_id_bytes: [u8; 32] = image_id.as_bytes().try_into().unwrap();
    let journal_bytes: [u8; 32] = journal.as_bytes().try_into().unwrap();

    let mut redeem_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    redeem_builder.append_r0_succinct_verifier_with_fixed_journal(image_id_bytes, control_id, None, journal_bytes).unwrap();
    let redeem_script = redeem_builder.drain();

    let mut sig_builder = ZkScriptBuilder::new_r0_with_flags(flags);
    sig_builder.push_r0_succinct_witness(receipt).unwrap();
    sig_builder.add_data(&redeem_script).unwrap();
    let sig_script = sig_builder.drain();

    execute_p2sh(sig_script, &redeem_script).unwrap();
}

#[test]
fn prepare_r0_groth16_proof_roundtrips() {
    let (_, _, receipt) = load_groth_fixture();
    let bytes = prepare_r0_groth16_proof(&receipt).unwrap();
    Proof::<Bn254>::deserialize_compressed(bytes.as_slice()).expect("prepared bytes must decode as a compressed proof");
    let bytes_again = prepare_r0_groth16_proof(&receipt).unwrap();
    assert_eq!(bytes, bytes_again, "prepare must be deterministic");
}
