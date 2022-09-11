mod uint;
construct_uint!(Uint256, 4);
construct_uint!(Uint320, 5);

impl From<Uint256> for Uint320 {
    fn from(u: Uint256) -> Self {
        let mut result = Uint320::ZERO;
        result.0[..4].copy_from_slice(&u.0);
        result
    }
}

impl core::convert::TryFrom<Uint320> for Uint256 {
    type Error = crate::uint::TryFromIntError;

    #[inline]
    fn try_from(value: Uint320) -> Result<Self, Self::Error> {
        if value.0[4] != 0 {
            Err(crate::uint::TryFromIntError(()))
        } else {
            let mut result = Uint256::ZERO;
            result.0.copy_from_slice(&value.0[..4]);
            Ok(result)
        }
    }
}
