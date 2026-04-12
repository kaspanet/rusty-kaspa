mod error;
use ark_bn254::{G1Affine, G2Affine};
use ark_serialize::CanonicalDeserialize;

pub trait PointFromBytes<'input>: Sized {
    type Input: ?Sized;
    fn from_bytes(bytes: &'input Self::Input) -> Result<Self, PointError>;
}
pub use error::PointError;
pub struct G1(pub G1Affine);
pub struct G2(pub G2Affine);

impl<'input> PointFromBytes<'input> for G1 {
    type Input = Vec<Vec<u8>>;

    /// Deserialize an element over the G1 group from bytes in big-endian format
    fn from_bytes(bytes: &Self::Input) -> Result<G1, PointError> {
        if bytes.len() != 2 {
            return Err(PointError::MalformedG1);
        }
        let g1_affine: Vec<u8> = bytes[0].iter().rev().chain(bytes[1].iter().rev()).cloned().collect();

        Ok(G1(G1Affine::deserialize_uncompressed(&*g1_affine)?))
    }
}

impl<'input> PointFromBytes<'input> for G2 {
    type Input = Vec<Vec<Vec<u8>>>;

    fn from_bytes(bytes: &Self::Input) -> Result<G2, PointError> {
        if bytes.len() != 2 || bytes[0].len() != 2 || bytes[1].len() != 2 {
            return Err(PointError::MalformedG2);
        }
        let g2_affine: Vec<u8> = bytes[0][1]
            .iter()
            .rev()
            .chain(bytes[0][0].iter().rev())
            .chain(bytes[1][1].iter().rev())
            .chain(bytes[1][0].iter().rev())
            .cloned()
            .collect();

        Ok(G2(G2Affine::deserialize_uncompressed(&*g2_affine)?))
    }
}

impl Into<G1Affine> for G1 {
    fn into(self) -> G1Affine {
        self.0
    }
}

impl Into<G2Affine> for G2 {
    fn into(self) -> G2Affine {
        self.0
    }
}