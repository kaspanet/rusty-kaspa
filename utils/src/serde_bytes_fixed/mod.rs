mod de;
mod ser;

pub use crate::serde_bytes_fixed::de::Deserialize;
pub use crate::serde_bytes_fixed::ser::Serialize;

pub fn serialize<T, S, const N: usize>(bytes: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: ?Sized + Serialize<N>,
    S: serde::Serializer,
{
    Serialize::serialize(bytes, serializer)
}

pub fn deserialize<'de, T, D, const N: usize>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de, N>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer)
}
