mod error;
use ark_bn254::{Bn254, G1Projective};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};

use borsh::{BorshDeserialize, BorshSerialize};
pub use error::Groth16Error;
use risc0_zkp::core::digest::Digest;

use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{error::ZkIntegrityError, ZkPrecompile},
};

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Fr(
    #[borsh(
        serialize_with = "borsh_ark::serialize",
        deserialize_with = "borsh_ark::deserialize"
    )]
    ark_bn254::Fr,
);
// Deserialize a scalar field from bytes in big-endian format
fn fr_from_bytes(scalar: &[u8]) -> Result<Fr, Groth16Error> {
    let scalar: Vec<u8> = scalar.iter().cloned().collect();
    Ok(Fr(ark_bn254::Fr::deserialize_uncompressed(&*scalar).map(|x| x).unwrap()))
}
pub struct Groth16Precompile;
impl ZkPrecompile for Groth16Precompile {
    type Error = Groth16Error;
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        let [unprepared_compressed_key] = dstack.pop_raw()?;
        let [proof_bytes] = dstack.pop_raw()?;
        let [n_inputs] = dstack.pop_raw()?;
        let n_inputs = u16::from_le_bytes(n_inputs.as_slice().try_into()?) as u16;
        let mut unprepared_public_inputs = Vec::new();
        for _ in 0..n_inputs {
            let [input_bytes] = dstack.pop_raw()?;
            unprepared_public_inputs.push(fr_from_bytes(&input_bytes)?);
        }
        println!("Groth16 Precompile: Starting verification");
        println!("Groth16 Precompile: unprepared key bytes len: {}", unprepared_compressed_key.len());
        let vk = VerifyingKey::deserialize_compressed(&*unprepared_compressed_key)?;
        println!("Groth16 Precompile: Deserialized verifying key");
        let pvk = ark_groth16::prepare_verifying_key(&vk);
        println!("Groth16 Precompile: Prepared verifying key");
        println!("proof bytes len: {}", proof_bytes.len());
        let proof: &Proof<ark_ec::bn::Bn<ark_bn254::Config>> = &Proof::deserialize_compressed(&*proof_bytes)?;

        println!("Groth16 Precompile: Deserialized proof");


         let mut encoded_prepared_inputs = Vec::new();
        let prepared_inputs = Groth16::<Bn254>::prepare_inputs(
            &pvk,
            &unprepared_public_inputs.iter().map(|x| x.0).collect::<Vec<_>>(),
        )?;
        prepared_inputs
            .serialize_compressed(&mut encoded_prepared_inputs)?;

        if Groth16::<Bn254>::verify_proof_with_prepared_inputs(&pvk, proof, &prepared_inputs)? {
            Ok(())
        } else {
            Err(Groth16Error::VerificationFailed)
        }
    }
}

/// A receipt composed of a Groth16 over the BN_254 curve.
/// This struct is a modified version of the Groth16Receipt defined in
/// risc0. The reason for this is to simplify it, as we are certain to only receive digests
/// for the claim and verifier parameters.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Inner {
    /// A Groth16 proof of a zkVM execution with the associated claim.
    seal: Vec<u8>,

    /// [ReceiptClaim][crate::ReceiptClaim] containing information about the execution that this
    /// receipt proves.
    claim: Digest,

    /// A digest of the verifier parameters that can be used to verify this receipt.
    ///
    /// Acts as a fingerprint to identify differing proof system or circuit versions between a
    /// prover and a verifier. Is not intended to contain the full verifier parameters, which must
    /// be provided by a trusted source (e.g. packaged with the verifier code).
    verifier_parameters: Digest,
}
mod borsh_ark {
    use alloc::vec::Vec;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use borsh::{BorshDeserialize, BorshSerialize};
    use std::io::{Read, Write};

    pub fn serialize<W: Write>(value: &impl CanonicalSerialize, writer: &mut W) -> std::io::Result<()> {
        let mut buffer = Vec::new();
        value.serialize_uncompressed(&mut buffer).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        writer.write_all(&buffer)
    }

    pub fn deserialize<R: Read, T: CanonicalDeserialize>(reader: &mut R) -> std::io::Result<T> {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        T::deserialize_uncompressed(buffer.as_slice()).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }
}


#[cfg(test)]
mod tests {
    use crate::{
        data_stack::Stack,
        zk_precompiles::{
            groth16::{Groth16Precompile, Inner},
            ZkPrecompile,
        },
    };

   

    #[test]
    fn try_verify_stack() {
        let unprepared_compressed_vk=hex::decode("e2f26dbea299f5223b646cb1fb33eadb059d9407559d7441dfd902e3a79a4d2dabb73dc17fbc13021e2471e0c08bd67d8401f52b73d6d07483794cad4778180e0c06f33bbc4c79a9cadef253a68084d382f17788f885c9afd176f7cb2f036789edf692d95cbdde46ddda5ef7d422436779445c5e66006a42761e1f12efde0018c212f3aeb785e49712e7a9353349aaf1255dfb31b7bf60723a480d9293938e1933033e7fea1f40604eaacf699d4be9aacc577054a0db22d9129a1728ff85a01a1c3af829b62bf4914c0bcf2c81a4bd577190eff5f194ee9bac95faefd53cb0030600000000000000e43bdc655d0f9d730535554d9caa611ddd152c081a06a932a8e1d5dc259aac123f42a188f683d869873ccc4c119442e57b056e03e2fa92f2028c97bc20b9078747c30f85444697fdf436e348711c011115963f855197243e4b39e6cbe236ca8ba7f2042e11f9255afbb6c6e2c3accb88e401f2aac21c097c92b3fbdb99f98a9b0dcd6c075ada6ed0ddfece1d4a2d005f61a7d5df0b75c18a5b2374d64e495fab93d4c4b1200394d5253cce2f25a59b862ee8e4cd43686603faa09d5d0d3c1c8f").unwrap();
        let proof=hex::decode("570253c0c483a1b16460118e63c155f3684e784ae7d97e8fc3f544128b37fe15075eab5ac31150c8a44253d8525971241bbd7227fcefbae2db4ae71675c56a2e0eb9235136b15ab72f16e707832f3d6ae5b0ba7cca53ae17cb52b3201919eb9d908c16297abd90aa7e00267bc21a9a78116e717d4d76edd44e21cca17e3d592d").unwrap();
        let input0=hex::decode("a54dc85ac99f851c92d7c96d7318af4100000000000000000000000000000000").unwrap();
        let input1=hex::decode("dbe7c0194edfcc37eb4d422a998c1f5600000000000000000000000000000000").unwrap();
        let input2=hex::decode("a95ac0b37bfedcd8136e6c1143086bf500000000000000000000000000000000").unwrap();
        let input3=hex::decode("d223ffcb21c6ffcb7c8f60392ca49dde00000000000000000000000000000000").unwrap();
        let input4=hex::decode("c07a65145c3cb48b6101962ea607a4dd93c753bb26975cb47feb00d3666e4404").unwrap();

        println!(
            "unprepared key len: {}, proof len: {}",
            unprepared_compressed_vk.len(),
            proof.len()
        );
        let mut stack = Stack::new();
        stack.push(input4);
        stack.push(input3);
        stack.push(input2);
        stack.push(input1);
        stack.push(input0);
        stack.push((5u16).to_le_bytes().to_vec());
        stack.push(proof);
        stack.push(unprepared_compressed_vk);
        let result = Groth16Precompile::verify_zk(&mut stack).unwrap();
    }
}
