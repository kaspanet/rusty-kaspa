pub mod error;

use ark_serialize::CanonicalDeserialize;

use crate::zk_precompiles::fields::error::FieldsError;

#[derive(Clone, Debug)]
pub struct Fr(ark_bn254::Fr);

impl Fr {
    pub fn field(&self) -> &ark_bn254::Fr {
        &self.0
    }
}

// Deserialize a scalar field from bytes in little-endian format
pub fn fr_from_bytes(scalar: &[u8]) -> Result<Fr, FieldsError> {
    let scalar: Vec<u8> = scalar.iter().cloned().collect();
    Ok(Fr(ark_bn254::Fr::deserialize_uncompressed(&*scalar).map(|x| x)?))
}
