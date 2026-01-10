mod error;
use ark_bn254::{Bn254, G1Projective};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof, VerifyingKey};
use ark_serialize::CanonicalDeserialize;

use borsh::{BorshDeserialize, BorshSerialize};
pub use error::Groth16Error;
use risc0_zkp::core::digest::Digest;

use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{error::ZkIntegrityError, ZkPrecompile},
};

pub struct Groth16Precompile;
impl ZkPrecompile for Groth16Precompile {
    type Error = Groth16Error;
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        let [unprepared_compressed_key] = dstack.pop_raw()?;
        let [proof_bytes] = dstack.pop_raw()?;
        let [prepared_inputs_bytes] = dstack.pop_raw()?;
        println!("Groth16 Precompile: Starting verification");
        println!("Groth16 Precompile: unprepared key bytes len: {}", unprepared_compressed_key.len());
        let vk = VerifyingKey::deserialize_compressed(&*unprepared_compressed_key)?;
        println!("Groth16 Precompile: Deserialized verifying key");
        let pvk = ark_groth16::prepare_verifying_key(&vk);
        println!("Groth16 Precompile: Prepared verifying key");
        println!("proof bytes len: {}", proof_bytes.len());
        let proof: &Proof<ark_ec::bn::Bn<ark_bn254::Config>> = &Proof::deserialize_compressed(&*proof_bytes)?; 

        println!("Groth16 Precompile: Deserialized proof");
        println!("prepared inputs bytes len: {}", prepared_inputs_bytes.len());
        let prepared_inputs = &G1Projective::deserialize_compressed(&*prepared_inputs_bytes)?;

        if Groth16::<Bn254>::verify_proof_with_prepared_inputs(&pvk, proof, prepared_inputs)? {
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

#[cfg(test)]
mod tests {
    use crate::{data_stack::Stack, zk_precompiles::{ZkPrecompile, groth16::{Groth16Precompile, Inner}}};

    use risc0_circuit_recursion::control_id::{ALLOWED_CONTROL_ROOT, BN254_IDENTITY_CONTROL_ID};
    use risc0_groth16::Verifier;
    use risc0_zkp::core::digest::Digest;

    // [1423791584, 880512669, 1738307172, 2533723364, 3880046003, 402541997, 1959133478, 277067013]
    #[test]
    fn hash_abcd() {
        let image_id_hex = "75641a540ee2ad9ee5902bcdcdb8b55c0bef4a28287309b858f97b1356c6c2e0";
        let journal_hex = "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456";
        let proof_data_hex = "0001000015fe378b1244f5c38f7ed9e74a784e68f355c1638e116064b1a183c4c0530257140cf216b9899d3a3fd8718db35946e75a5e69a2b4884935cd98d35c624c6ae41deb191920b352cb17ae53ca7cbab0e56a3d2f8307e7162fb75ab1365123b90e2e6ac57516e74adbe2baeffc2772bd1b24715952d85342a4c85011c35aab5e0728f81402ffe3655b3d07fe0a3df01a9b959ed54d2dccd4a955b77aa2ad08a1d103a01eb634d8f7ccb2ab903e053a0e0960a5b22f2d70d17f98dcb1936e940c2b2d593d7ea1cc214ed4ed764d7d716e11789a1ac27b26007eaa90bd7a29168c90142e64de0dfd31ffc775f3a5a31f87ff42cf78de195f8b78c3ea43f8b9a2cce1a95ac0b37bfedcd8136e6c1143086bf5d223ffcb21c6ffcb7c8f60392ca49dde73c457ba541936f0d907daf0c7253a39a9c5c427c225ba7709e44702d3c6eedc";
        // Parse hex strings and convert to bytes
        let image_id_bytes = hex::decode(image_id_hex).unwrap();
        let journal_bytes = hex::decode(journal_hex).unwrap();
        let proof_data_bytes = hex::decode(proof_data_hex).unwrap();

        // The journal here is a digest of the public outputs of the ZK program.
        // Do note here that the R0 precompiles are verifying the executions of the
        // lift program, which verifies a previous ZK proof and outputs its public outputs as journal.
        // Rest assured, the integrity of the journal is still bound by the proof.

        // Deserialize the proof and prepare for verification
        let inner: Inner = borsh::from_slice(&proof_data_bytes).unwrap();

        // Convert image id and journal to Digests
        let image_id: Digest = Digest::try_from(image_id_bytes.as_slice()).unwrap();
        let journal: Digest = Digest::try_from(journal_bytes.as_slice()).unwrap();
        let verifier =
            Verifier::new(&inner.seal, ALLOWED_CONTROL_ROOT, inner.claim, BN254_IDENTITY_CONTROL_ID, &risc0_groth16::verifying_key())
                .unwrap();

        let encoded_pvk_hex = hex::encode(verifier.unprepared_key);
        let encoded_proof = hex::encode(verifier.encoded_proof);
        let encoded_prepared_inputs = hex::encode(verifier.encoded_prepared_inputs);
        println!("pvk: {:?},{}", encoded_pvk_hex, encoded_pvk_hex.len());
        println!("proof: {:?}", encoded_proof);
        println!("input: {:?}", encoded_prepared_inputs);
    }


    #[test]
    fn try_verify_stack() {
        let unprepared_compressed_vk=hex::decode("e2f26dbea299f5223b646cb1fb33eadb059d9407559d7441dfd902e3a79a4d2dabb73dc17fbc13021e2471e0c08bd67d8401f52b73d6d07483794cad4778180e0c06f33bbc4c79a9cadef253a68084d382f17788f885c9afd176f7cb2f036789edf692d95cbdde46ddda5ef7d422436779445c5e66006a42761e1f12efde0018c212f3aeb785e49712e7a9353349aaf1255dfb31b7bf60723a480d9293938e1933033e7fea1f40604eaacf699d4be9aacc577054a0db22d9129a1728ff85a01a1c3af829b62bf4914c0bcf2c81a4bd577190eff5f194ee9bac95faefd53cb0030600000000000000e43bdc655d0f9d730535554d9caa611ddd152c081a06a932a8e1d5dc259aac123f42a188f683d869873ccc4c119442e57b056e03e2fa92f2028c97bc20b9078747c30f85444697fdf436e348711c011115963f855197243e4b39e6cbe236ca8ba7f2042e11f9255afbb6c6e2c3accb88e401f2aac21c097c92b3fbdb99f98a9b0dcd6c075ada6ed0ddfece1d4a2d005f61a7d5df0b75c18a5b2374d64e495fab93d4c4b1200394d5253cce2f25a59b862ee8e4cd43686603faa09d5d0d3c1c8f").unwrap();
        let proof=hex::decode("570253c0c483a1b16460118e63c155f3684e784ae7d97e8fc3f544128b37fe15075eab5ac31150c8a44253d8525971241bbd7227fcefbae2db4ae71675c56a2e0eb9235136b15ab72f16e707832f3d6ae5b0ba7cca53ae17cb52b3201919eb9d908c16297abd90aa7e00267bc21a9a78116e717d4d76edd44e21cca17e3d592d").unwrap();
        let prepared_input=hex::decode("5bd47174120aeb1ea35e913a041e868ef48c1f8936f5a173cbf3bbeafc89121a").unwrap();
        println!("unprepared key len: {}, proof len: {}, input len: {}", unprepared_compressed_vk.len(), proof.len(), prepared_input.len());
        let mut stack=Stack::new();
        stack.push(prepared_input);
        stack.push(proof);
        stack.push(unprepared_compressed_vk);
        let result=Groth16Precompile::verify_zk(&mut stack).unwrap();
    }
}
