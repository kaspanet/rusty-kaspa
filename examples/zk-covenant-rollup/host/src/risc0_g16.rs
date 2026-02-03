use anyhow::anyhow;
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use crate::script_ext::ScriptBuilderExt;
use kaspa_txscript::opcodes::codes::{
    OpCat, OpFromAltStack, OpRot, OpSHA256, OpSwap, OpToAltStack, OpVerify, OpZkPrecompile,
};
use kaspa_txscript::script_builder::ScriptBuilder;
use risc0_groth16::Seal;
use risc0_zkvm::{Digest, Groth16ReceiptVerifierParameters};

pub const PUBLIC_INPUT_COUNT: usize = 5;

pub trait Risc0Groth16Verify {
    /// Expects on stack: [compressed_proof, journal_hash, program_id]
    /// Computes receipt_claim from program_id and journal_hash, builds the full
    /// Groth16 verification stack using proof from the stack, and verifies.
    fn verify_risc0_groth16(&mut self) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder>;

    /// Expects [journal_hash, program_id] on stack. Computes receipt_claim hash using opcodes.
    fn compute_receipt_claim(&mut self) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder>;
}

impl Risc0Groth16Verify for ScriptBuilder {
    fn verify_risc0_groth16(&mut self) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
        let (a0, a1) = control_root_split();

        // Stack: [proof, journal_hash, program_id]
        self.add_op(OpRot)?;
        self.add_op(OpToAltStack)?;

        self.compute_receipt_claim()?;

        self.add_op(OpToAltStack)?;
        self.add_data(id_bn254_fr_uncompressed().as_ref())?;
        self.add_op(OpFromAltStack)?;
        self.split_at_mid()?;

        self.add_data(&[0; 16])?;
        self.add_op(OpCat)?;
        self.add_op(OpSwap)?;
        self.add_data(&[0; 16])?;
        self.add_op(OpCat)?;

        self.add_data(&a1)?;
        self.add_data(&a0)?;
        self.add_u16(PUBLIC_INPUT_COUNT as u16)?;
        self.add_op(OpFromAltStack)?;
        self.add_data(&verifying_key_compressed())?;
        self.add_data(&[0x20u8])?;
        self.add_op(OpZkPrecompile)?;
        self.add_op(OpVerify)
    }

    fn compute_receipt_claim(&mut self) -> kaspa_txscript::script_builder::ScriptBuilderResult<&mut ScriptBuilder> {
        // Stack: [journal_hash, program_id]
        self
            .add_op(OpToAltStack)?
            .add_data(&OUTPUT_TAG_DIGEST)?
            .add_op(OpSwap)?
            .add_op(OpCat)?
            .add_data(&[0u8; 32])?
            .add_op(OpCat)?
            .add_data(&2u16.to_le_bytes())?
            .add_op(OpCat)?
            .add_op(OpSHA256)?

            .add_data(&RECEIPT_CLAIM_TAG_DIGEST)?
            .add_data(&[0u8; 32])?
            .add_op(OpCat)?
            .add_op(OpFromAltStack)?
            .add_op(OpCat)?
            .add_data(&POST_DIGEST)?
            .add_op(OpCat)?
            .add_op(OpSwap)?
            .add_op(OpCat)?
            .add_data(&[0u8, 0, 0, 0, 0, 0, 0, 0, 4, 0])?
            .add_op(OpCat)?
            .add_op(OpSHA256)
    }
}

/// Converts a risc0 Groth16 seal to compressed arkworks proof bytes.
pub fn seal_to_compressed_proof(seal: &[u8]) -> Vec<u8> {
    let seal = Seal::decode(seal).unwrap();
    let proof = Proof::<Bn254> {
        a: g1_from_bytes(&seal.a).unwrap(),
        b: g2_from_bytes(&seal.b).unwrap(),
        c: g1_from_bytes(&seal.c).unwrap(),
    };
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

fn g1_from_bytes(elem: &[Vec<u8>]) -> Result<G1Affine, anyhow::Error> {
    if elem.len() != 2 {
        return Err(anyhow!("Malformed G1 field element"));
    }
    let g1_affine: Vec<u8> = elem[0].iter().rev().chain(elem[1].iter().rev()).cloned().collect();
    G1Affine::deserialize_uncompressed(&*g1_affine).map_err(|err| anyhow!(err))
}

fn g2_from_bytes(elem: &[Vec<Vec<u8>>]) -> Result<G2Affine, anyhow::Error> {
    if elem.len() != 2 || elem[0].len() != 2 || elem[1].len() != 2 {
        return Err(anyhow!("Malformed G2 field element"));
    }
    let g2_affine: Vec<u8> = elem[0][1]
        .iter().rev()
        .chain(elem[0][0].iter().rev())
        .chain(elem[1][1].iter().rev())
        .chain(elem[1][0].iter().rev())
        .cloned()
        .collect();
    G2Affine::deserialize_uncompressed(&*g2_affine).map_err(|err| anyhow!(err))
}

const POST_DIGEST: [u8; 32] = [
    163, 172, 194, 113, 23, 65, 137, 150, 52, 11, 132, 229, 169, 15, 62, 244, 196, 157, 34, 199,
    158, 68, 170, 216, 34, 236, 156, 49, 62, 30, 184, 226,
];

const OUTPUT_TAG_DIGEST: [u8; 32] = [
    119, 234, 254, 179, 102, 167, 139, 71, 116, 125, 224, 215, 187, 23, 98, 132, 8, 95, 245, 86,
    72, 135, 0, 154, 91, 230, 61, 163, 45, 53, 89, 212,
];

const RECEIPT_CLAIM_TAG_DIGEST: [u8; 32] = [
    203, 31, 239, 205, 31, 45, 154, 100, 151, 92, 187, 191, 110, 22, 30, 41, 20, 67, 75, 12, 187,
    153, 96, 184, 77, 245, 215, 23, 232, 107, 72, 175,
];
