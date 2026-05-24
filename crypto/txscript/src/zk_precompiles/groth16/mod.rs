mod error;
use ark_bn254::{Bn254, G1Affine, G2Affine};
use ark_groth16::{Groth16, Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, Compress, Valid, Validate};
use kaspa_consensus_core::mass::ScriptUnits;

pub use error::Groth16Error;

use crate::{
    EngineFlags,
    data_stack::Stack,
    opcodes::i32s_to_usizes,
    runtime_resource_meter::RuntimeResourceMeter,
    zk_precompiles::{
        ZkPrecompile,
        fields::{Fr, TruncFr},
    },
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

    if !reader.is_empty() {
        return Err(Groth16Error::TrailingVerifyingKeyBytes);
    }
    <G1Affine as Valid>::batch_check(gamma_abc_g1.iter())?;

    Ok(VerifyingKey { alpha_g1, beta_g2, gamma_g2, delta_g2, gamma_abc_g1 })
}

pub struct Groth16Precompile;
impl ZkPrecompile for Groth16Precompile {
    type Error = Groth16Error;
    /// Verifies the integrity of a Groth16 proof.
    ///
    /// *NOTE: Experimental code; not yet fully audited for mainnet use.* TODO(pre-covpp)
    ///
    fn verify_zk(dstack: &mut Stack, meter: &mut RuntimeResourceMeter, flags: EngineFlags) -> Result<(), Self::Error> {
        // Retrieve the compressed VK
        let [unprepared_compressed_key] = dstack.pop_raw()?;

        // Retrieve compressed proof
        let [proof_bytes] = dstack.pop_raw()?;

        // Retrieve number of public inputs
        let [n_inputs] = i32s_to_usizes(dstack.pop_items::<1, i32>()?)?;

        // Retrieve public inputs
        // Do not change the capacity argument to allow arbitrary input, as
        // this would allow an adversary to cause OOM. The actual remaining stack
        // length is an upper bound on how many inputs can be read.
        let mut unprepared_public_inputs = Vec::with_capacity(n_inputs.min(dstack.len()));

        // For each public input, pop from the stack and convert to Fr.
        //
        // Note: public input count is bounded by the script stack depth limit.
        for _ in 0..n_inputs {
            // convert bytes to Fr according to whether we're in hardened mode or not
            let fr = if flags.zk_hardening_enabled {
                let [fr] = dstack.pop_items::<1, Fr>()?;
                fr
            } else {
                let [trunc_fr] = dstack.pop_items::<1, TruncFr>()?;
                Fr::from(trunc_fr)
            };
            unprepared_public_inputs.push(fr.into_field());
        }

        // Deserialize the verifying key. Post-activation: streamed deserialize
        // that arity-checks and meters per gamma_abc_g1 element inline. Pre-
        // activation: plain ark deserialize plus the historical empty-gamma_abc
        // check, no per-element charge.
        let vk = if flags.zk_hardening_enabled {
            deserialize_verifying_key_with_metering(&unprepared_compressed_key, unprepared_public_inputs.len(), meter)?
        } else {
            let vk = VerifyingKey::deserialize_compressed(&*unprepared_compressed_key)?;
            if vk.gamma_abc_g1.is_empty() {
                return Err(Groth16Error::EmptyGammaAbc);
            }
            vk
        };

        // Prepare verifying key
        let pvk = ark_groth16::prepare_verifying_key(&vk);

        // Deserialize proof
        let mut proof_reader = proof_bytes.as_slice();
        let proof = Proof::<Bn254>::deserialize_compressed(&mut proof_reader)?;
        if flags.zk_hardening_enabled && !proof_reader.is_empty() {
            return Err(Groth16Error::TrailingProofBytes);
        }

        // Prepare public inputs with the prepared verifying key
        let prepared_inputs = Groth16::<Bn254>::prepare_inputs(&pvk, &unprepared_public_inputs)?;

        // Verify the proof with the prepared inputs
        if Groth16::<Bn254>::verify_proof_with_prepared_inputs(&pvk, &proof, &prepared_inputs)? {
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
        EngineFlags,
        data_stack::Stack,
        runtime_resource_meter::RuntimeResourceMeter,
        zk_precompiles::{ZkPrecompile, groth16::Groth16Precompile, tests::helpers::load_groth_fields},
    };
    use ark_bn254::{Bn254, G1Affine, G2Affine};
    use ark_groth16::VerifyingKey;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress};
    use kaspa_consensus_core::mass::ScriptUnits;
    use kaspa_txscript_errors::TxScriptError;

    fn hardened_flags() -> EngineFlags {
        EngineFlags { covenants_enabled: true, zk_hardening_enabled: true, ..Default::default() }
    }

    fn legacy_flags() -> EngineFlags {
        EngineFlags { covenants_enabled: true, zk_hardening_enabled: false, ..Default::default() }
    }

    fn stack_with_groth_fields(vk: Vec<u8>, proof: Vec<u8>, inputs: Vec<Vec<u8>>) -> Stack {
        let mut stack = Stack::new(Vec::new(), true);
        for input in inputs.iter().rev() {
            stack.push(input.clone().into()).unwrap();
        }
        stack.push_item(inputs.len() as i32).unwrap();
        stack.push(proof.into()).unwrap();
        stack.push(vk.into()).unwrap();
        stack
    }

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
        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter, hardened_flags()).expect_err("arity mismatch must be rejected");
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

        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter, hardened_flags()).expect_err("over-budget VK must be rejected");
        match err {
            Groth16Error::FromTxScript(TxScriptError::ExceededCommittedScriptUnits { used, limit }) => {
                assert_eq!(limit, PER_INPUT_BUDGET.0);
                assert_eq!(used, expected_charge);
            }
            other => panic!("expected ExceededCommittedScriptUnits for gamma_abc_g1 element count = {COUNT}, got: {other:?}"),
        }
    }

    #[test]
    fn hardened_verify_path_accepts_canonical_proof() {
        let (vk, proof, inputs) = load_groth_fields();
        let mut stack = stack_with_groth_fields(vk, proof, inputs);
        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        Groth16Precompile::verify_zk(&mut stack, &mut meter, hardened_flags()).unwrap();
    }

    #[test]
    fn legacy_verify_path_accepts_canonical_proof_without_metering() {
        let (vk, proof, inputs) = load_groth_fields();
        let mut stack = stack_with_groth_fields(vk, proof, inputs);

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(0));
        Groth16Precompile::verify_zk(&mut stack, &mut meter, legacy_flags()).expect("legacy path must accept canonical proof");
        assert_eq!(meter.used_script_units(), ScriptUnits(0), "legacy path must not consume meter units");
    }

    #[test]
    fn legacy_verify_path_tolerates_oversized_fr_push() {
        let (vk, proof, mut inputs) = load_groth_fields();
        inputs[0].extend_from_slice(&[0xAB; 32]);
        let mut stack = stack_with_groth_fields(vk, proof, inputs);

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        Groth16Precompile::verify_zk(&mut stack, &mut meter, legacy_flags())
            .expect("legacy path must accept 64-byte Fr push (silent truncation)");
    }

    #[test]
    fn legacy_verify_path_tolerates_trailing_vk_and_proof_bytes() {
        let (mut vk, mut proof, inputs) = load_groth_fields();
        vk.push(0xAB);
        proof.push(0xCD);
        let mut stack = stack_with_groth_fields(vk, proof, inputs);

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        Groth16Precompile::verify_zk(&mut stack, &mut meter, legacy_flags()).expect("legacy path must accept trailing VK/proof bytes");
    }

    #[test]
    fn hardened_verify_path_rejects_oversized_fr_push() {
        let vk_bytes = vk_with_gamma_abc_count(6); // 5 pub inputs + 1
        let oversized_input = vec![0u8; 64];

        let mut stack = Stack::new(Vec::new(), true);
        for _ in 0..4 {
            stack.push(vec![0u8; 32].into()).unwrap();
        }
        stack.push(oversized_input.into()).unwrap(); // 64-byte push, must be rejected
        stack.push_item(5i32).unwrap();
        stack.push(vec![0u8; 128].into()).unwrap();
        stack.push(vk_bytes.into()).unwrap();

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter, hardened_flags())
            .expect_err("hardened path must reject oversized Fr push");
        match err {
            Groth16Error::FromTxScript(TxScriptError::ZkIntegrity(msg)) if msg.contains("Invalid Fr length") => {}
            other => panic!("expected Invalid Fr length error, got: {other:?}"),
        }
    }

    #[test]
    fn hardened_verify_path_rejects_trailing_vk_bytes() {
        let (mut vk, proof, inputs) = load_groth_fields();
        vk.push(0xAB);
        let mut stack = stack_with_groth_fields(vk, proof, inputs);

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter, hardened_flags())
            .expect_err("hardened path must reject trailing VK bytes");
        match err {
            Groth16Error::TrailingVerifyingKeyBytes => {}
            other => panic!("expected trailing VK error, got: {other:?}"),
        }
    }

    #[test]
    fn hardened_verify_path_rejects_trailing_proof_bytes() {
        let (vk, mut proof, inputs) = load_groth_fields();
        proof.push(0xCD);
        let mut stack = stack_with_groth_fields(vk, proof, inputs);

        let mut meter = RuntimeResourceMeter::new_script_units(ScriptUnits(0), ScriptUnits(u64::MAX));
        let err = Groth16Precompile::verify_zk(&mut stack, &mut meter, hardened_flags())
            .expect_err("hardened path must reject trailing proof bytes");
        match err {
            Groth16Error::TrailingProofBytes => {}
            other => panic!("expected trailing proof error, got: {other:?}"),
        }
    }
}
