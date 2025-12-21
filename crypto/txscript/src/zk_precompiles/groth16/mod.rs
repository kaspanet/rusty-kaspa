mod error;
use ark_bn254::{Bn254, G1Projective};
use ark_groth16::{Groth16, PreparedVerifyingKey, Proof};
use ark_serialize::CanonicalDeserialize;


pub use error::Groth16Error;

use crate::{
    data_stack::{DataStack, Stack},
    zk_precompiles::{
        ZkPrecompile, error::ZkIntegrityError,
    },
};

pub struct Groth16Precompile;
impl ZkPrecompile for Groth16Precompile {
    type Error = Groth16Error;
    fn verify_zk(dstack: &mut Stack) -> Result<(), Self::Error> {
        if Groth16::<Bn254>::verify_proof_with_prepared_inputs(
            &PreparedVerifyingKey::deserialize_uncompressed(&*dstack.pop_raw::<1>()?[0])?,
            &Proof::deserialize_uncompressed(&*dstack.pop_raw::<1>()?[0])?,
            &G1Projective::deserialize_uncompressed(dstack.pop_raw::<1>()?[0].as_slice())?,
        )? {
            Ok(())
        } else {
            Err(Groth16Error::VerificationFailed)
        }
    }
}