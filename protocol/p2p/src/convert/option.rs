//!
//! Traits to allow safe inline conversion over types such as Option (which are not owned by this crate, so cannot impl ordinary `TryFrom`)
//!
use super::error::ConversionError;

pub(crate) trait TryFromOptionEx<T>: Sized {
    type Error;
    fn try_from_ex(value: T) -> Result<Self, Self::Error>;
}

impl<T, U: TryFrom<T, Error = ConversionError>> TryFromOptionEx<Option<T>> for U {
    type Error = ConversionError;

    fn try_from_ex(value: Option<T>) -> Result<Self, Self::Error> {
        if let Some(inner) = value {
            Ok(inner.try_into()?)
        } else {
            Err(ConversionError::NoneValue)
        }
    }
}

impl<'a, T: 'a, U: TryFrom<&'a T, Error = ConversionError>> TryFromOptionEx<&'a Option<T>> for U {
    type Error = ConversionError;

    fn try_from_ex(value: &'a Option<T>) -> Result<Self, Self::Error> {
        if let Some(inner) = value {
            Ok(inner.try_into()?)
        } else {
            Err(ConversionError::NoneValue)
        }
    }
}

pub(crate) trait TryIntoOptionEx<T>: Sized {
    type Error;
    fn try_into_ex(self) -> Result<T, Self::Error>;
}

impl<T, U> TryIntoOptionEx<U> for T
where
    U: TryFromOptionEx<T>,
{
    type Error = U::Error;
    fn try_into_ex(self) -> Result<U, U::Error> {
        U::try_from_ex(self)
    }
}
