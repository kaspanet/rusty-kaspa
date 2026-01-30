use anyhow::anyhow;
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use kaspa_txscript::opcodes::codes::{
    Op2Dup, OpCat, OpDiv, OpDup, OpFromAltStack, OpMul, OpRot, OpSHA256, OpSize, OpSubstr, OpSwap, OpToAltStack, OpVerify,
    OpZkPrecompile,
};
use kaspa_txscript::script_builder::ScriptBuilder;
use risc0_groth16::Seal;
use risc0_zkvm::{Digest, Groth16ReceiptVerifierParameters};

pub const PUBLIC_INPUT_COUNT: usize = 5;

/// Expects on stack: [compressed_proof, journal_hash, program_id]
/// Computes receipt_claim from program_id and journal_hash, builds the full
/// Groth16 verification stack using proof from the stack, and verifies.
///
/// Stack layout for OpZkPrecompile (Groth16 tag 0x20), bottom to top:
///   [id_bn254_fr, c1_padded, c0_padded, a1, a0, 5, proof, vk, 0x20]
pub fn apply_to_builder(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    let (a0, a1) = control_root_split();

    // Stack: [proof, journal_hash, program_id]

    // Move proof to alt stack
    builder.add_op(OpRot)?; // [journal_hash, program_id, proof]
    builder.add_op(OpToAltStack)?; // [journal_hash, program_id], alt: [proof]

    // Compute receipt_claim from program_id and journal_hash (consumes both)
    receipt_claim_to_builder(builder)?; // [receipt_claim_hash], alt: [proof]

    // The groth16 precompile pops inputs and builds the vector in pop order.
    // risc0 expects public inputs as [a0, a1, c0, c1, id_bn254_fr].
    // So we must push in reverse: id at bottom, a0 on top.

    builder.add_op(OpToAltStack)?; // [], alt: [proof, claim]
    builder.add_data(id_bn254_fr_uncompressed().as_ref())?; // [id]
    builder.add_op(OpFromAltStack)?; // [id, claim], alt: [proof]
    split_at_mid(builder)?; // [id, claim_left, claim_right]

    // Pad claim_right (on top)
    builder.add_data(&[0; 16])?;
    builder.add_op(OpCat)?; // [id, claim_left, c1_padded]
                            // Pad claim_left
    builder.add_op(OpSwap)?; // [id, c1_padded, claim_left]
    builder.add_data(&[0; 16])?;
    builder.add_op(OpCat)?; // [id, c1_padded, c0_padded]
                            // Push control root halves
    builder.add_data(&a1)?; // [id, c1, c0, a1]
    builder.add_data(&a0)?; // [id, c1, c0, a1, a0]
    builder.add_u16(PUBLIC_INPUT_COUNT as u16)?; // [..., 5]
                                                 // Bring proof from alt stack
    builder.add_op(OpFromAltStack)?; // [..., 5, proof], alt: []
    builder.add_data(&verifying_key_compressed())?; // [..., 5, proof, vk]
    builder.add_data(&[0x20u8])?; // [..., 5, proof, vk, 0x20]
    builder.add_op(OpZkPrecompile)?; // [true]
    builder.add_op(OpVerify) // []
}

/// Converts a risc0 Groth16 seal to compressed arkworks proof bytes.
pub fn seal_to_compressed_proof(seal: &[u8]) -> Vec<u8> {
    let seal = Seal::decode(seal).unwrap();
    let proof =
        Proof::<Bn254> { a: g1_from_bytes(&seal.a).unwrap(), b: g2_from_bytes(&seal.b).unwrap(), c: g1_from_bytes(&seal.c).unwrap() };
    let mut proof_compressed = Vec::new();
    proof.serialize_compressed(&mut proof_compressed).unwrap();
    proof_compressed
}

fn verifying_key_compressed() -> Vec<u8> {
    let vk_risc0 = risc0_groth16::verifying_key();
    let vk_bytes_prefixed_with_len = bincode::serialize(&vk_risc0).unwrap();

    let vk = VerifyingKey::<Bn254>::deserialize_uncompressed(&mut &vk_bytes_prefixed_with_len[8..]).unwrap();
    let mut vk_compressed_bytes = Vec::new();
    vk.serialize_compressed(&mut vk_compressed_bytes).unwrap();
    vk_compressed_bytes
}

fn id_bn254_fr_uncompressed() -> impl AsRef<[u8]> {
    Groth16ReceiptVerifierParameters::default().bn254_control_id
}

fn split_at_mid(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // todo it may use split opcode natively

    builder
        .add_op(OpSize)?// [[1,2,3,4,5,6], 6 ]
        .add_i64(2)?// [[1,2,3,4,5,6], 6, 2 ]
        .add_op(OpDiv)?// [[1,2,3,4,5,6], 3 ]
        .add_op(Op2Dup)?  // [[1,2,3,4,5,6], 3, [1,2,3,4,5,6], 3 ]

        .add_i64(0)?// [[1,2,3,4,5,6], 3, [1,2,3,4,5,6], 3, 0 ]
        .add_op(OpSwap)?
        .add_op(OpSubstr)?  // [[1,2,3,4,5,6], 3, [1,2,3]]
        .add_op(OpRot)? // [3, [1,2,3], [1,2,3,4,5,6]]
        .add_op(OpRot)? // [[1,2,3], [1,2,3,4,5,6], 3]

        .add_op(OpDup)? // [[1,2,3], [1,2,3,4,5,6], 3, 3]
        .add_i64(2)? // [[1,2,3], [1,2,3,4,5,6], 3, 3, 2]
        .add_op(OpMul)? // [[1,2,3], [1,2,3,4,5,6], 6, 3]
        .add_op(OpSubstr)?  // [[1,2,3], [4,5,6]]
    ;

    Ok(builder)
}

fn control_root_split() -> ([u8; 32], [u8; 32]) {
    let id = Groth16ReceiptVerifierParameters::default().control_root;
    split_digest(id)
}

fn split_digest(d: Digest) -> ([u8; 32], [u8; 32]) {
    const MID: usize = size_of::<Digest>() / 2;

    let (first_half, second_half) = d.as_bytes().split_at(MID);
    let [a, b] = [first_half, second_half].map(|slice| {
        let mut scalar = [0u8; 32];
        scalar[..MID].copy_from_slice(slice);
        scalar
    });

    (a, b)
}

/// Deserialize an element over the G1 group from bytes in big-endian format
pub(crate) fn g1_from_bytes(elem: &[Vec<u8>]) -> Result<G1Affine, anyhow::Error> {
    if elem.len() != 2 {
        return Err(anyhow!("Malformed G1 field element"));
    }
    let g1_affine: Vec<u8> = elem[0].iter().rev().chain(elem[1].iter().rev()).cloned().collect();

    G1Affine::deserialize_uncompressed(&*g1_affine).map_err(|err| anyhow!(err))
}

/// Deserialize an element over the G2 group from bytes in big-endian format
pub(crate) fn g2_from_bytes(elem: &[Vec<Vec<u8>>]) -> Result<G2Affine, anyhow::Error> {
    if elem.len() != 2 || elem[0].len() != 2 || elem[1].len() != 2 {
        return Err(anyhow!("Malformed G2 field element"));
    }
    let g2_affine: Vec<u8> = elem[0][1]
        .iter()
        .rev()
        .chain(elem[0][0].iter().rev())
        .chain(elem[1][1].iter().rev())
        .chain(elem[1][0].iter().rev())
        .cloned()
        .collect();

    G2Affine::deserialize_uncompressed(&*g2_affine).map_err(|err| anyhow!(err))
}

/// Expects [journal_hash, program_id] on stack. Computes receipt_claim hash using opcodes.
/// Equivalent to `receipt_claim(journal_hash, image_id)` but done on-stack.
fn receipt_claim_to_builder(builder: &mut ScriptBuilder) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
    // Stack: [journal_hash, program_id]

    // --- Compute output_digest = SHA256(OUTPUT_TAG || journal_hash || zeros32 || 2u16_le) ---

    builder
        .add_op(OpToAltStack)? // [journal_hash], alt: [program_id]
        .add_data(&OUTPUT_TAG_DIGEST)?          // [journal_hash, OUTPUT_TAG], alt: [program_id]
        .add_op(OpSwap)?                        // [OUTPUT_TAG, journal_hash], alt: [program_id]
        .add_op(OpCat)?                         // [OUTPUT_TAG || journal_hash]  (64 bytes), alt: [program_id]
        .add_data(&[0u8; 32])?                  // [OUTPUT_TAG || journal_hash, zeros32], alt: [program_id]
        .add_op(OpCat)?                         // [OUTPUT_TAG || journal_hash || zeros32]  (96 bytes), alt: [program_id]
        .add_data(&2u16.to_le_bytes())?         // [..., 0x0200], alt: [program_id]
        .add_op(OpCat)?                         // [98-byte output preimage], alt: [program_id]
        .add_op(OpSHA256)?                      // [output_digest], alt: [program_id]

    // --- Compute receipt_claim = SHA256(TAG || input || image_id || POST || output_digest || trailer) ---

        .add_data(&RECEIPT_CLAIM_TAG_DIGEST)?   // [output_digest, TAG], alt: [program_id]
        .add_data(&[0u8; 32])?             // [output_digest, TAG, input(zeros)], alt: [program_id]
        .add_op(OpCat)?                         // [output_digest, TAG || input]  (64 bytes), alt: [program_id]

        .add_op(OpFromAltStack)?                   // [output_digest, TAG || input, image_id]
        .add_op(OpCat)?                         // [output_digest, TAG || input || image_id]  (96 bytes)
        .add_data(&POST_DIGEST)?                // [output_digest, TAG || input || image_id, POST]
        .add_op(OpCat)?                         // [output_digest, TAG || input || image_id || POST]  (128 bytes)
        .add_op(OpSwap)?                        // [TAG || input || image_id || POST, output_digest]
        .add_op(OpCat)?                         // [TAG || ... || POST || output_digest]  (160 bytes)
        .add_data(&[0u8, 0, 0, 0, 0, 0, 0, 0, 4, 0])?  // [..., trailer(exit_codes + len)]
        .add_op(OpCat)?                         // [170-byte receipt_claim preimage]
        .add_op(OpSHA256) // [receipt_claim_hash]
}

const POST_DIGEST: [u8; 32] = [
    163, 172, 194, 113, 23, 65, 137, 150, 52, 11, 132, 229, 169, 15, 62, 244, 196, 157, 34, 199, 158, 68, 170, 216, 34, 236, 156, 49,
    62, 30, 184, 226,
];

const OUTPUT_TAG_DIGEST: [u8; 32] = [
    119, 234, 254, 179, 102, 167, 139, 71, 116, 125, 224, 215, 187, 23, 98, 132, 8, 95, 245, 86, 72, 135, 0, 154, 91, 230, 61, 163,
    45, 53, 89, 212,
];

const RECEIPT_CLAIM_TAG_DIGEST: [u8; 32] = [
    203, 31, 239, 205, 31, 45, 154, 100, 151, 92, 187, 191, 110, 22, 30, 41, 20, 67, 75, 12, 187, 153, 96, 184, 77, 245, 215, 23, 232,
    107, 72, 175,
];

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        hashing::sighash::SigHashReusedValuesUnsync,
        tx::{Transaction, TransactionInput, UtxoEntry, VerifiableTransaction},
    };
    use kaspa_txscript::{
        caches::Cache,
        opcodes::codes::{OpEqual, OpEqualVerify},
        zk_precompiles::risc0::receipt_claim::{Output, ReceiptClaim},
        EngineFlags, TxScriptEngine,
    };
    use risc0_zkvm::{
        sha::rust_crypto::{Digest as _, Sha256},
        sha::Digestible,
        ExitCode, SystemState,
    };

    fn output_digest(journal_hash: [u8; 32]) -> [u8; 32] {
        let mut preimage = [0u8; 98];
        preimage[..32].copy_from_slice(&OUTPUT_TAG_DIGEST);
        preimage[32..64].copy_from_slice(&journal_hash);
        preimage[96..98].copy_from_slice(&2u16.to_le_bytes());

        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let output_digest: [u8; 32] = *hasher.finalize().as_array().unwrap();
        output_digest
    }

    fn receipt_claim(journal_hash: [u8; 32], image_id: [u8; 32]) -> [u8; 32] {
        let mut preimage = [0u8; 170];

        preimage[..32].copy_from_slice(&RECEIPT_CLAIM_TAG_DIGEST); // tag digest
        preimage[32..64].copy_from_slice(&[0u8; 32]); // input
        preimage[64..96].copy_from_slice(&image_id); // pre
        preimage[96..128].copy_from_slice(&POST_DIGEST); //post
        preimage[128..160].copy_from_slice(&output_digest(journal_hash)); // output
        preimage[160..170].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 4, 0]); // exit codes and len of prev downs

        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let output_digest: [u8; 32] = *hasher.finalize().as_array().unwrap();
        output_digest
    }

    type Result<T> = kaspa_txscript::script_builder::ScriptBuilderResult<T>;

    #[derive(Clone)]
    struct VerifiableTransactionMock;

    impl VerifiableTransaction for VerifiableTransactionMock {
        fn tx(&self) -> &Transaction {
            unimplemented!()
        }

        fn populated_input(&self, _index: usize) -> (&TransactionInput, &UtxoEntry) {
            unimplemented!()
        }
        fn utxo(&self, _index: usize) -> Option<&UtxoEntry> {
            unimplemented!()
        }
    }

    #[test]
    fn test_verifying_key_compressed() {
        let vk_compressed = verifying_key_compressed();
    }

    #[test]
    fn test_receipt_claim_tag() {
        let mut hasher = Sha256::new();
        hasher.update("risc0.ReceiptClaim".as_bytes());
        let digest: [u8; 32] = *hasher.finalize().as_array().unwrap();
        assert_eq!(RECEIPT_CLAIM_TAG_DIGEST, digest);
    }

    #[test]
    fn test_post_digest() {
        assert_eq!(POST_DIGEST, SystemState { pc: 0, merkle_root: Default::default() }.digest().as_bytes());
    }

    #[test]
    fn test_output_digest() {
        let journal_hash = [123u8; 32];
        assert_eq!(
            output_digest(journal_hash),
            Output { journal: journal_hash.into(), assumptions: Default::default() }.digest().as_bytes()
        );
    }
    #[test]
    fn test_receipt_claim() {
        let actual = receipt_claim([1; 32], [255; 32]);
        let expected = ReceiptClaim {
            pre: [255; 32].into(),
            post: SystemState { pc: 0, merkle_root: Digest::ZERO },
            exit_code: ExitCode::Halted(0),
            input: Digest::ZERO,
            output: Output { journal: [1; 32].into(), assumptions: Digest::ZERO },
        }
        .digest();

        assert_eq!(actual, expected.as_bytes());
    }
    #[test]
    fn test_receipt_claim_to_builder() -> Result<()> {
        let sig_cache = Cache::new(10_000);
        let reused_values = SigHashReusedValuesUnsync::new();

        let journal_hash = [1u8; 32];
        let image_id = [255u8; 32];
        let expected = receipt_claim(journal_hash, image_id);

        let mut builder = ScriptBuilder::new();
        builder.add_data(&journal_hash)?.add_data(&image_id)?;

        let script = receipt_claim_to_builder(&mut builder)?.add_data(&expected)?.add_op(OpEqual)?.drain();

        let mut engine: TxScriptEngine<VerifiableTransactionMock, _> =
            TxScriptEngine::from_script(&script, &reused_values, &sig_cache, EngineFlags { covenants_enabled: true });
        engine.execute().unwrap();
        Ok(())
    }

    #[test]
    fn test_split_digest() -> Result<()> {
        let sig_cache = Cache::new(10_000);
        let reused_values = SigHashReusedValuesUnsync::new();

        let mut builder = ScriptBuilder::new();
        builder.add_data(&[1, 2, 3, 4, 5, 6])?;

        let script = split_at_mid(&mut builder)?
            .add_data(&[4, 5, 6])?
            .add_op(OpEqualVerify)?
            .add_data(&[1, 2, 3])
            .unwrap()
            .add_op(OpEqual)?
            .drain();

        let mut engine: TxScriptEngine<VerifiableTransactionMock, _> =
            TxScriptEngine::from_script(&script, &reused_values, &sig_cache, EngineFlags { covenants_enabled: true });
        engine.execute().unwrap();
        Ok(())
    }
}
