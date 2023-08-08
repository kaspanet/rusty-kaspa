use crate::hex::FromHex;
use serde::Deserializer;
use std::fmt::Display;

pub trait Deserialize<'de>: Sized + FromHex + TryFrom<&'de [u8]> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

impl<'de, T: FromHex + TryFrom<&'de [u8]>> Deserialize<'de> for T
where
    <T as TryFrom<&'de [u8]>>::Error: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s: &str = serde::Deserialize::deserialize(deserializer)?;
            Ok(T::from_hex(s).map_err(serde::de::Error::custom)?)
        } else {
            // serde::Deserialize for &[u8] is already optimized, so simply forward to that.
            serde::Deserialize::deserialize(deserializer).and_then(|bts| T::try_from(bts).map_err(serde::de::Error::custom))
        }
    }
}
