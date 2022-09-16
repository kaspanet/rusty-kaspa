pub mod int;
pub mod uint;
construct_uint!(Uint256, 4);
construct_uint!(Uint320, 5);
construct_uint!(Uint3072, 48);

impl From<Uint256> for Uint320 {
    #[inline]
    fn from(u: Uint256) -> Self {
        let mut result = Uint320::ZERO;
        result.0[..4].copy_from_slice(&u.0);
        result
    }
}

impl TryFrom<Uint320> for Uint256 {
    type Error = crate::uint::TryFromIntError;

    #[inline]
    fn try_from(value: Uint320) -> Result<Self, Self::Error> {
        if value.0[4] != 0 {
            Err(crate::uint::TryFromIntError)
        } else {
            let mut result = Uint256::ZERO;
            result.0.copy_from_slice(&value.0[..4]);
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Uint256;

    #[test]
    fn test_overflow_bug() {
        let a = Uint256::from_le_bytes([
            255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255,
        ]);
        let b = Uint256::from_le_bytes([
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 71, 33, 0, 0, 0, 0, 0, 0,
            0, 32, 0, 0, 0,
        ]);
        let c = a.overflowing_add(b).0;
        let expected = [254, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 71, 33, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0];
        assert_eq!(c.to_le_bytes(), expected);
    }
}
