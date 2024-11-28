use crate::hex::FromHex;
use std::{fmt::Display, str};

pub trait Deserialize<'de>: Sized + FromHex + TryFrom<&'de [u8]> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>;
}

impl<'de, T: FromHex + TryFrom<&'de [u8]>> Deserialize<'de> for T
where
    <T as TryFrom<&'de [u8]>>::Error: Display,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            deserializer.deserialize_str(FromHexVisitor::default())
        } else {
            // serde::Deserialize for &[u8] is already optimized, so simply forward to that.
            serde::Deserialize::deserialize(deserializer).and_then(|bts| T::try_from(bts).map_err(serde::de::Error::custom))
        }
    }
}

pub struct FromHexVisitor<'de, T: FromHex> {
    marker: std::marker::PhantomData<T>,
    lifetime: std::marker::PhantomData<&'de ()>,
}

impl<T: FromHex> Default for FromHexVisitor<'_, T> {
    fn default() -> Self {
        Self { marker: Default::default(), lifetime: Default::default() }
    }
}

impl<'de, T: FromHex> serde::de::Visitor<'de> for FromHexVisitor<'de, T> {
    type Value = T;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "string, str or slice, vec of bytes")
    }
    #[inline]
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        FromHex::from_hex(v).map_err(serde::de::Error::custom)
    }

    #[inline]
    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        FromHex::from_hex(v).map_err(serde::de::Error::custom)
    }

    #[inline]
    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        FromHex::from_hex(&v).map_err(serde::de::Error::custom)
    }

    #[inline]
    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let str = str::from_utf8(v).map_err(serde::de::Error::custom)?;
        FromHex::from_hex(str).map_err(serde::de::Error::custom)
    }

    #[inline]
    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let str = str::from_utf8(v).map_err(serde::de::Error::custom)?;
        FromHex::from_hex(str).map_err(serde::de::Error::custom)
    }

    #[inline]
    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let str = str::from_utf8(&v).map_err(serde::de::Error::custom)?;
        FromHex::from_hex(str).map_err(serde::de::Error::custom)
    }
}
