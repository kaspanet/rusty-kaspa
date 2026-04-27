pub mod error;

use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use kaspa_txscript_errors::SerializationError;
use smallvec::SmallVec;

use crate::{
    TxScriptError,
    data_stack::{OpcodeData, StackEntry},
    zk_precompiles::fields::error::FieldsError,
};

#[derive(Clone, Debug)]
pub struct Fr(ark_bn254::Fr);

impl Fr {
    pub fn field(&self) -> &ark_bn254::Fr {
        &self.0
    }
}

impl TryInto<Vec<u8>> for Fr {
    type Error = FieldsError;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let mut bytes = Vec::new();
        self.0.serialize_uncompressed(&mut bytes).map_err(|e| FieldsError::ArkSerialization(e))?;
        Ok(bytes)
    }
}

impl TryFrom<&[u8]> for Fr {
    type Error = FieldsError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        Ok(Fr(ark_bn254::Fr::deserialize_uncompressed(bytes)?))
    }
}

impl TryFrom<Vec<u8>> for Fr {
    type Error = FieldsError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Fr::try_from(bytes.as_slice())
    }
}

impl OpcodeData<Fr> for StackEntry {
    fn deserialize(&self, _: bool) -> Result<Fr, TxScriptError> {
        Fr::try_from(self.as_slice()).map_err(|e| TxScriptError::ZkIntegrity(e.to_string()))
    }

    fn serialize(from: &Fr) -> Result<Self, SerializationError> {
        let mut bytes = Vec::new();
        from.0.serialize_uncompressed(&mut bytes).map_err(|_| SerializationError::ArkSerialization)?;
        Ok(SmallVec::from_vec(bytes))
    }
}
