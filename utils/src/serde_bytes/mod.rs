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
