use crate::hex::FromHex;
use serde::Deserializer;

pub trait Deserialize<'de>: Sized + FromHex + From<&'de [u8]> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

impl<'de, T: FromHex + From<&'de [u8]>> Deserialize<'de> for T {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s: &str = serde::Deserialize::deserialize(deserializer)?;
            Ok(T::from_hex(s).map_err(serde::de::Error::custom)?)
        } else {
            // serde::Deserialize for &[u8] is already optimized, so simply forward to that.
            let bytes: &[u8] = serde::Deserialize::deserialize(deserializer)?;
            Ok(T::from(bytes))
        }
    }
}
