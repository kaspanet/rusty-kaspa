mod error;
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_groth16::{Groth16, Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, Compress, Valid, Validate};
use kaspa_consensus_core::mass::ScriptUnits;

pub use error::Groth16Error;

use crate::{
    data_stack::Stack,
    opcodes::i32s_to_usizes,
    runtime_resource_meter::RuntimeResourceMeter,
    zk_precompiles::{ZkPrecompile, fields::Fr},
};

/// Empirically determined script unit cost per gamma_abc_g1 element in the VK
/// such that the total verification cost is within 10ms.
pub const GROTH16_GAMMA_ABC_G1_ELEMENT_SCRIPT_UNITS: u64 = 60_000;

fn deserialize_verifying_key_with_metering(
    bytes: &[u8],
    public_input_count: usize,
    meter: &mut RuntimeResourceMeter,
) -> Result<VerifyingKey<Bn254>, Groth16Error> {
    let mut reader = bytes;

    // Mirror ark-groth16's VerifyingKey serialization order, but stop after
    // gamma_abc_g1 length so we can check arity and charge before reading it.
    let alpha_g1 = G1Affine::deserialize_with_mode(&mut reader, Compress::Yes, Validate::Yes)?;
    let beta_g2 = G2Affine::deserialize_with_mode(&mut reader, Compress::Yes, Validate::Yes)?;
    let gamma_g2 = G2Affine::deserialize_with_mode(&mut reader, Compress::Yes, Validate::Yes)?;
    let delta_g2 = G2Affine::deserialize_with_mode(&mut reader, Compress::Yes, Validate::Yes)?;

    let gamma_abc_element_count = u64::deserialize_with_mode(&mut reader, Compress::Yes, Validate::Yes)?;

    // Covered by the following count check but kept for clearer error
    if gamma_abc_element_count == 0 {
        return Err(Groth16Error::EmptyGammaAbc);
    }

    // Public inputs are stack-depth bounded, so +1 cannot overflow.
    if public_input_count as u64 + 1 != gamma_abc_element_count {
        return Err(ark_relations::gr1cs::SynthesisError::ArityMismatch.into());
    }

    let gamma_abc_cost = ScriptUnits(gamma_abc_element_count.saturating_mul(GROTH16_GAMMA_ABC_G1_ELEMENT_SCRIPT_UNITS));

    // Try consuming the vk cost and err if we are over the limit
    meter.consume_script_units(gamma_abc_cost)?;

    let gamma_abc_len = public_input_count + 1;
    let gamma_abc_g1 = (0..gamma_abc_len)
        .map(|_| G1Affine::deserialize_with_mode(&mut reader, Compress::Yes, Validate::No))
        .collect::<Result<Vec<_>, _>>()?;

    <G1Affine as Valid>::batch_check(gamma_abc_g1.iter())?;

    Ok(VerifyingKey { alpha_g1, beta_g2, gamma_g2, delta_g2, gamma_abc_g1 })
}

pub struct Groth16Precompile;
impl ZkPrecompile for Groth16Precompile {
    type Error = Groth16Error;
    /// Verifies the integrity of a Groth16 proof.
    ///
    /// *NOTE: Experimental code; not yet fully audited for mainnet use.* TODO(pre-covpp)
    fn verify_zk(dstack: &mut Stack, meter: &mut RuntimeResourceMeter) -> Result<(), Self::Error> {
        // Retrieve the compressed VK
        let [unprepared_compressed_key] = dstack.pop_raw()?;

        // Retrieve compressed proof
        let [proof_bytes] = dstack.pop_raw()?;

        // Retrieve number of public inputs
        let [n_inputs] = i32s_to_usizes(dstack.pop_items::<1, i32>()?)?;

        // Retrieve public inputs
        let mut unprepared_public_inputs = Vec::new();

        // For each public input, pop from the stack and convert to Fr.
        //
        // Note: public input count is bounded by the script stack depth limit.
        for _ in 0..n_inputs {
            let [fr] = dstack.pop_items::<1, Fr>()?;
            // Convert bytes to Fr and add to public inputs
            unprepared_public_inputs.push(fr);
        }

        // Deserialize verifying key
        let vk = deserialize_verifying_key_with_metering(&unprepared_compressed_key, unprepared_public_inputs.len(), meter)?;

        // Prepare verifying key
        let pvk = ark_groth16::prepare_verifying_key(&vk);

        // Deserialize proof
        let proof: &Proof<ark_ec::bn::Bn<ark_bn254::Config>> = &Proof::deserialize_compressed(&*proof_bytes)?;

        // Prepare public inputs with the prepared verifying key
        let prepared_inputs =
            Groth16::<Bn254>::prepare_inputs(&pvk, &unprepared_public_inputs.iter().map(|x| *x.field()).collect::<Vec<_>>())?;

        // Verify the proof with the prepared inputs
        if Groth16::<Bn254>::verify_proof_with_prepared_inputs(&pvk, proof, &prepared_inputs)? {
            Ok(())
        } else {
            Err(Groth16Error::VerificationFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{GROTH16_GAMMA_ABC_G1_ELEMENT_SCRIPT_UNITS, Groth16Error};
    use crate::{
        data_stack::Stack,
        hex,
        runtime_resource_meter::RuntimeResourceMeter,
        zk_precompiles::{ZkPrecompile, groth16::Groth16Precompile},
    };
    use ark_bn254::{Bn254, G1Affine, G2Affine};
    use ark_groth16::VerifyingKey;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress};
    use kaspa_consensus_core::mass::ScriptUnits;
    use kaspa_txscript_errors::TxScriptError;

    #[test]
    fn check_sizes() {
        assert_eq!(G1Affine::default().serialized_size(Compress::Yes), 32);
        assert_eq!(G2Affine::default().serialized_size(Compress::Yes), 64);
    }

    #[test]
    fn check_vec_prefix() {
        let v: Vec<u8> = vec![];
        let mut buf = Vec::new();
        v.serialize_compressed(&mut buf).unwrap();
        assert_eq!(buf.len(), 8); // empty Vec serializes to just the length prefix
        assert_eq!(buf, [0u8; 8]); // length 0 as LE u64

        let v: Vec<u8> = vec![0xAA];
        let mut buf = Vec::new();
        v.serialize_compressed(&mut buf).unwrap();
        assert_eq!(&buf[..8], &[1, 0, 0, 0, 0, 0, 0, 0]); // length 1 LE u64
        assert_eq!(buf[8], 0xAA);
    }

    fn vk_with_gamma_abc_count(count: usize) -> Vec<u8> {
        let vk = VerifyingKey::<Bn254> {
            alpha_g1: G1Affine::default(),
            beta_g2: G2Affine::default(),
            gamma_g2: G2Affine::default(),
            delta_g2: G2Affine::default(),
            gamma_abc_g1: vec![G1Affine::default(); count],
        };
        let mut bytes = Vec::new();
        vk.serialize_compressed(&mut bytes).expect("serialize VK");
        bytes
    }

    #[test]
    fn custom_vk_deserialize_matches_ark() {
        for &gamma_abc_count in &[1usize, 2, 6, 42] {
            let vk_bytes = vk_with_gamma_abc_count(gamma_abc_count);
            let ark_vk = VerifyingKey::<Bn254>::deserialize_compressed(&*vk_bytes).expect("Ark should deserialize VK");

            let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
            let custom_vk = super::deserialize_verifying_key_with_metering(&vk_bytes, gamma_abc_count - 1, &mut meter)
                .expect("custom VK deserializer should match Ark");

            assert_eq!(
                meter.used_script_units(),
                ScriptUnits((gamma_abc_count as u64).saturating_mul(GROTH16_GAMMA_ABC_G1_ELEMENT_SCRIPT_UNITS))
            );
            assert_eq!(custom_vk, ark_vk);
        }
    }

    #[test]
    fn verify_zk_rejects_arity_mismatch_before_meter_charge() {
        let vk_bytes = vk_with_gamma_abc_count(5);

        let mut stack = Stack::new(Vec::new(), true);
        stack.push_item(0i32).unwrap();
        stack.push(vec![0u8; 128].into()).unwrap();
        stack.push(vk_bytes.into()).unwrap();

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(0));
        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter).expect_err("arity mismatch must be rejected");
        match err {
            Groth16Error::ArkR1CS(ark_relations::gr1cs::SynthesisError::ArityMismatch) => {}
            other => panic!("expected ArityMismatch before meter charge, got: {other:?}"),
        }
        assert_eq!(meter.used_script_units(), ScriptUnits(0));
    }

    #[test]
    fn verify_zk_rejects_over_budget_vk_via_meter() {
        const PER_INPUT_BUDGET: ScriptUnits = ScriptUnits(200_000);
        const COUNT: usize = 5;
        let vk_bytes = vk_with_gamma_abc_count(COUNT);

        let mut stack = Stack::new(Vec::new(), true);
        for _ in 0..COUNT - 1 {
            stack.push(vec![0u8; 32].into()).unwrap();
        }
        stack.push_item((COUNT - 1) as i32).unwrap();
        stack.push(vec![0u8; 128].into()).unwrap();
        stack.push(vk_bytes.into()).unwrap();

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), PER_INPUT_BUDGET);

        let expected_charge = (COUNT as u64).saturating_mul(GROTH16_GAMMA_ABC_G1_ELEMENT_SCRIPT_UNITS);
        assert!(expected_charge > PER_INPUT_BUDGET.0, "gamma_abc charge {expected_charge} must exceed budget {}", PER_INPUT_BUDGET.0);

        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter).expect_err("over-budget VK must be rejected");
        match err {
            Groth16Error::FromTxScript(TxScriptError::ExceededCommittedScriptUnits { used, limit }) => {
                assert_eq!(limit, PER_INPUT_BUDGET.0);
                assert_eq!(used, expected_charge);
            }
            other => panic!("expected ExceededCommittedScriptUnits for gamma_abc_g1 element count = {COUNT}, got: {other:?}"),
        }
    }

    #[test]
    fn try_verify_stack() {
        let unprepared_compressed_vk=hex::decode("e2f26dbea299f5223b646cb1fb33eadb059d9407559d7441dfd902e3a79a4d2dabb73dc17fbc13021e2471e0c08bd67d8401f52b73d6d07483794cad4778180e0c06f33bbc4c79a9cadef253a68084d382f17788f885c9afd176f7cb2f036789edf692d95cbdde46ddda5ef7d422436779445c5e66006a42761e1f12efde0018c212f3aeb785e49712e7a9353349aaf1255dfb31b7bf60723a480d9293938e1933033e7fea1f40604eaacf699d4be9aacc577054a0db22d9129a1728ff85a01a1c3af829b62bf4914c0bcf2c81a4bd577190eff5f194ee9bac95faefd53cb0030600000000000000e43bdc655d0f9d730535554d9caa611ddd152c081a06a932a8e1d5dc259aac123f42a188f683d869873ccc4c119442e57b056e03e2fa92f2028c97bc20b9078747c30f85444697fdf436e348711c011115963f855197243e4b39e6cbe236ca8ba7f2042e11f9255afbb6c6e2c3accb88e401f2aac21c097c92b3fbdb99f98a9b0dcd6c075ada6ed0ddfece1d4a2d005f61a7d5df0b75c18a5b2374d64e495fab93d4c4b1200394d5253cce2f25a59b862ee8e4cd43686603faa09d5d0d3c1c8f").unwrap();
        let proof=hex::decode("570253c0c483a1b16460118e63c155f3684e784ae7d97e8fc3f544128b37fe15075eab5ac31150c8a44253d8525971241bbd7227fcefbae2db4ae71675c56a2e0eb9235136b15ab72f16e707832f3d6ae5b0ba7cca53ae17cb52b3201919eb9d908c16297abd90aa7e00267bc21a9a78116e717d4d76edd44e21cca17e3d592d").unwrap();
        let input0 = hex::decode("a54dc85ac99f851c92d7c96d7318af4100000000000000000000000000000000").unwrap();
        let input1 = hex::decode("dbe7c0194edfcc37eb4d422a998c1f5600000000000000000000000000000000").unwrap();
        let input2 = hex::decode("a95ac0b37bfedcd8136e6c1143086bf500000000000000000000000000000000").unwrap();
        let input3 = hex::decode("d223ffcb21c6ffcb7c8f60392ca49dde00000000000000000000000000000000").unwrap();
        let input4 = hex::decode("c07a65145c3cb48b6101962ea607a4dd93c753bb26975cb47feb00d3666e4404").unwrap();

        println!("unprepared key len: {}, proof len: {}", unprepared_compressed_vk.len(), proof.len());

        let mut stack = Stack::new(Vec::new(), true);
        stack.push(input4.into()).unwrap();
        stack.push(input3.into()).unwrap();
        stack.push(input2.into()).unwrap();
        stack.push(input1.into()).unwrap();
        stack.push(input0.into()).unwrap();
        stack.push_item(5i32).unwrap(); // Number of public inputs
        stack.push(proof.into()).unwrap();
        stack.push(unprepared_compressed_vk.into()).unwrap();
        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        Groth16Precompile::verify_zk(&mut stack, &mut meter).unwrap();
    }
}
