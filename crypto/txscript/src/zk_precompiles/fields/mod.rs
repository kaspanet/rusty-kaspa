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

pub const FR_BYTES: usize = 32;

impl TryFrom<&[u8]> for Fr {
    type Error = FieldsError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() != FR_BYTES {
            return Err(FieldsError::InvalidLength(bytes.len()));
        }
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

#[cfg(test)]
mod tests {
    use super::{FR_BYTES, Fr};
    use crate::zk_precompiles::fields::error::FieldsError;

    #[test]
    fn fr_accepts_exactly_32_bytes() {
        let mut bytes = [0u8; FR_BYTES];
        bytes[0] = 0x01;
        Fr::try_from(bytes.as_slice()).expect("32-byte in-range Fr must parse");
    }

    #[test]
    fn fr_rejects_oversized_buffer() {
        let oversized = vec![0u8; 64];
        match Fr::try_from(oversized.as_slice()) {
            Err(FieldsError::InvalidLength(64)) => {}
            other => panic!("expected InvalidLength(64), got {other:?}"),
        }
    }

    #[test]
    fn fr_rejects_undersized_buffer() {
        let short = vec![0u8; 16];
        match Fr::try_from(short.as_slice()) {
            Err(FieldsError::InvalidLength(16)) => {}
            other => panic!("expected InvalidLength(16), got {other:?}"),
        }
    }
}
