use kaspa_consensus_core::{
    hashing::sighash::SigHashReusedValuesUnsync,
    tx::{PopulatedTransaction, Transaction, TransactionId, TransactionInput, TransactionOutpoint, UtxoEntry},
};
use kaspa_txscript_errors::TxScriptError;

use crate::{
    EngineCtx, EngineFlags, SigCacheKey, TxScriptEngine,
    caches::Cache,
    hex,
    opcodes::codes::OpZkPrecompile,
    pay_to_script_hash_script, pay_to_script_hash_signature_script_with_flags,
    script_builder::{ScriptBuilder, ScriptBuilderResult},
    zk_precompiles::tags::ZkTag,
};

pub fn zk_test_flags() -> EngineFlags {
    EngineFlags { covenants_enabled: true, zk_hardening_enabled: true, ..Default::default() }
}

pub fn build_zk_script(elements: &[&[u8]]) -> ScriptBuilderResult<Vec<u8>> {
    let mut builder = ScriptBuilder::with_flags(EngineFlags { covenants_enabled: true, ..Default::default() });
    for element in elements {
        builder.add_data(element)?;
    }
    builder.add_op(OpZkPrecompile)?;
    Ok(builder.drain())
}

pub fn execute_zk_script(
    script: &[u8],
    sig_cache: &Cache<SigCacheKey, bool>,
    reused_values: &SigHashReusedValuesUnsync,
) -> Result<(), TxScriptError> {
    execute_zk_script_with_flags(
        script,
        sig_cache,
        reused_values,
        EngineFlags { covenants_enabled: true, zk_hardening_enabled: true, ..Default::default() },
    )
}

pub fn execute_zk_script_with_flags(
    script: &[u8],
    sig_cache: &Cache<SigCacheKey, bool>,
    reused_values: &SigHashReusedValuesUnsync,
    flags: EngineFlags,
) -> Result<(), TxScriptError> {
    let mut vm =
        TxScriptEngine::<PopulatedTransaction, SigHashReusedValuesUnsync>::from_script(script, reused_values, sig_cache, flags);
    vm.execute()
}

pub fn execute_p2sh_script(
    signature_script: Vec<u8>,
    redeem_script: &[u8],
    sig_cache: &Cache<SigCacheKey, bool>,
    reused_values: &SigHashReusedValuesUnsync,
) -> Result<(), TxScriptError> {
    let outpoint = TransactionOutpoint::new(TransactionId::default(), 0);
    let input = TransactionInput::new_with_compute_budget(outpoint, signature_script, 0, 0);
    let utxo = UtxoEntry::new(20_000, pay_to_script_hash_script(redeem_script), 0, false, None);
    let tx = Transaction::new(1, vec![input], vec![], 0, Default::default(), 0, vec![]);
    let populated_tx = PopulatedTransaction::new(&tx, vec![utxo.clone()]);
    let mut vm = TxScriptEngine::from_transaction_input(
        &populated_tx,
        &populated_tx.tx.inputs[0],
        0,
        &utxo,
        EngineCtx::new(sig_cache).with_reused(reused_values),
        zk_test_flags(),
    );

    vm.execute()
}

#[allow(clippy::type_complexity)]
pub fn load_stark_fields() -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let stark_seal_hex = include_str!("data/succinct.seal.hex");
    let stark_control_id_hex = include_str!("data/succinct.control_id.hex");
    let stark_claim_hex = include_str!("data/succinct.claim.hex");
    let stark_hashfn_hex = include_str!("data/succinct.hashfn.hex");
    let stark_control_index_hex = include_str!("data/succinct.control_index.hex");
    let stark_control_digests_hex = include_str!("data/succinct.control_digests.hex");
    let stark_image_id_hex = include_str!("data/succinct.image.hex");
    let stark_journal_hex = include_str!("data/succinct.journal.hex");

    let stark_control_id_bytes = hex::decode(stark_control_id_hex).expect("Failed to decode hex STARK control id");
    let stark_seal_bytes = hex::decode(stark_seal_hex).expect("Failed to decode hex STARK seal");
    let stark_claim_bytes = hex::decode(stark_claim_hex).expect("Failed to decode hex STARK claim");
    let stark_hashfn_bytes = hex::decode(stark_hashfn_hex).expect("Failed to decode hex STARK hashfn");
    let stark_control_index_bytes = hex::decode(stark_control_index_hex).expect("Failed to decode hex STARK control index");
    let stark_control_digests_bytes = hex::decode(stark_control_digests_hex).expect("Failed to decode hex STARK control digests");
    let stark_image_id_bytes = hex::decode(stark_image_id_hex).expect("Failed to decode hex image id");
    let stark_journal_bytes = hex::decode(stark_journal_hex).expect("Failed to decode hex journal");

    (
        stark_control_id_bytes,
        stark_seal_bytes,
        stark_claim_bytes,
        stark_hashfn_bytes,
        stark_control_index_bytes,
        stark_control_digests_bytes,
        stark_journal_bytes,
        stark_image_id_bytes,
    )
}

#[derive(Clone, Debug)]
pub struct R0Fields {
    pub claim: Vec<u8>,
    pub control_index: Vec<u8>,
    pub control_digests: Vec<u8>,
    pub seal: Vec<u8>,
    pub journal: Vec<u8>,
    pub image_id: Vec<u8>,
    pub control_id: Vec<u8>,
    pub hashfn: Vec<u8>,
}

impl R0Fields {
    pub fn from_fixture() -> Self {
        let (control_id, seal, claim, hashfn, control_index, control_digests, journal, image_id) = load_stark_fields();
        Self { claim, control_index, control_digests, seal, journal, image_id, control_id, hashfn }
    }

    pub fn script(&self) -> Vec<u8> {
        build_zk_script(&[
            &self.claim,
            &self.control_index,
            &self.control_digests,
            &self.seal,
            &self.journal,
            &self.image_id,
            &self.control_id,
            &self.hashfn,
            &[ZkTag::R0Succinct as u8],
        ])
        .unwrap()
    }

    pub fn p2sh_redeem_script(&self) -> Vec<u8> {
        let mut builder = ScriptBuilder::with_flags(zk_test_flags());
        builder
            .add_data(&self.image_id)
            .unwrap()
            .add_data(&self.control_id)
            .unwrap()
            .add_data(&self.hashfn)
            .unwrap()
            .add_data(&[ZkTag::R0Succinct as u8])
            .unwrap()
            .add_op(OpZkPrecompile)
            .unwrap()
            .drain()
    }

    pub fn p2sh_signature_script(&self, redeem_script: Vec<u8>) -> Vec<u8> {
        let mut signature = ScriptBuilder::with_flags(zk_test_flags());
        signature
            .add_data(&self.claim)
            .unwrap()
            .add_data(&self.control_index)
            .unwrap()
            .add_data(&self.control_digests)
            .unwrap()
            .add_data(&self.seal)
            .unwrap()
            .add_data(&self.journal)
            .unwrap();

        pay_to_script_hash_signature_script_with_flags(redeem_script, signature.drain(), zk_test_flags()).unwrap()
    }

    pub fn p2sh_scripts(&self) -> (Vec<u8>, Vec<u8>) {
        let redeem_script = self.p2sh_redeem_script();
        let signature_script = self.p2sh_signature_script(redeem_script.clone());
        (signature_script, redeem_script)
    }
}

pub fn build_stark_script(break_control_id: bool) -> Vec<u8> {
    let mut fields = R0Fields::from_fixture();
    if break_control_id {
        fields.control_id[0] ^= 0xFF;
    }
    fields.script()
}

pub fn build_groth_script() -> Vec<u8> {
    let (unprepared_compressed_vk, groth16_proof_bytes, inputs) = load_groth_fields();
    build_groth_script_from_fields(&unprepared_compressed_vk, &groth16_proof_bytes, &inputs)
}

pub fn build_groth_script_from_fields(unprepared_compressed_vk: &[u8], groth16_proof_bytes: &[u8], inputs: &[Vec<u8>]) -> Vec<u8> {
    let groth16_tag = ZkTag::Groth16 as u8;
    let mut builder = ScriptBuilder::with_flags(EngineFlags { covenants_enabled: true, ..Default::default() });
    for input in inputs.iter().rev() {
        builder.add_data(input).unwrap();
    }
    builder
        .add_i64(inputs.len() as i64)
        .unwrap()
        .add_data(groth16_proof_bytes)
        .unwrap()
        .add_data(unprepared_compressed_vk)
        .unwrap()
        .add_data(&[groth16_tag])
        .unwrap()
        .add_op(OpZkPrecompile)
        .unwrap()
        .drain()
}

pub fn load_groth_fields() -> (Vec<u8>, Vec<u8>, Vec<Vec<u8>>) {
    let unprepared_compressed_vk = hex::decode("e2f26dbea299f5223b646cb1fb33eadb059d9407559d7441dfd902e3a79a4d2dabb73dc17fbc13021e2471e0c08bd67d8401f52b73d6d07483794cad4778180e0c06f33bbc4c79a9cadef253a68084d382f17788f885c9afd176f7cb2f036789edf692d95cbdde46ddda5ef7d422436779445c5e66006a42761e1f12efde0018c212f3aeb785e49712e7a9353349aaf1255dfb31b7bf60723a480d9293938e1933033e7fea1f40604eaacf699d4be9aacc577054a0db22d9129a1728ff85a01a1c3af829b62bf4914c0bcf2c81a4bd577190eff5f194ee9bac95faefd53cb0030600000000000000e43bdc655d0f9d730535554d9caa611ddd152c081a06a932a8e1d5dc259aac123f42a188f683d869873ccc4c119442e57b056e03e2fa92f2028c97bc20b9078747c30f85444697fdf436e348711c011115963f855197243e4b39e6cbe236ca8ba7f2042e11f9255afbb6c6e2c3accb88e401f2aac21c097c92b3fbdb99f98a9b0dcd6c075ada6ed0ddfece1d4a2d005f61a7d5df0b75c18a5b2374d64e495fab93d4c4b1200394d5253cce2f25a59b862ee8e4cd43686603faa09d5d0d3c1c8f").unwrap();
    let groth16_proof_bytes = hex::decode("570253c0c483a1b16460118e63c155f3684e784ae7d97e8fc3f544128b37fe15075eab5ac31150c8a44253d8525971241bbd7227fcefbae2db4ae71675c56a2e0eb9235136b15ab72f16e707832f3d6ae5b0ba7cca53ae17cb52b3201919eb9d908c16297abd90aa7e00267bc21a9a78116e717d4d76edd44e21cca17e3d592d").unwrap();
    let input0 = hex::decode("a54dc85ac99f851c92d7c96d7318af4100000000000000000000000000000000").unwrap();
    let input1 = hex::decode("dbe7c0194edfcc37eb4d422a998c1f5600000000000000000000000000000000").unwrap();
    let input2 = hex::decode("a95ac0b37bfedcd8136e6c1143086bf500000000000000000000000000000000").unwrap();
    let input3 = hex::decode("d223ffcb21c6ffcb7c8f60392ca49dde00000000000000000000000000000000").unwrap();
    let input4 = hex::decode("c07a65145c3cb48b6101962ea607a4dd93c753bb26975cb47feb00d3666e4404").unwrap();

    (unprepared_compressed_vk, groth16_proof_bytes, vec![input0, input1, input2, input3, input4])
}
