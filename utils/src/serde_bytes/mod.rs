mod de;
mod ser;

pub use crate::serde_bytes::de::{Deserialize, FromHexVisitor};
pub use crate::serde_bytes::ser::Serialize;

pub fn serialize<T, S>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ?Sized + Serialize,
    S: serde::Serializer,
{
    Serialize::serialize(bytes, serializer)
}
pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer)
}

/// Borrowed byte-slice newtype that serializes through this module's helpers.
/// Mirrors `serde_bytes::Bytes` from the upstream crate.
#[repr(transparent)]
pub struct Bytes<'a>(pub &'a [u8]);

impl serde::Serialize for Bytes<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serialize(&self.0, serializer)
    }
}

/// Owned byte-buffer newtype that deserializes through this module's helpers.
/// Mirrors `serde_bytes::ByteBuf` from the upstream crate.
#[derive(serde::Deserialize)]
#[repr(transparent)]
pub struct ByteBuf(#[serde(with = "crate::serde_bytes")] pub alloc::vec::Vec<u8>);
