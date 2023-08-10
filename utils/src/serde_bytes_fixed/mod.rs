mod de;
mod ser;

pub use crate::serde_bytes_fixed::de::Deserialize;
pub use crate::serde_bytes_fixed::ser::Serialize;
use crate::{serde_impl_deser_fixed_bytes, serde_impl_ser_fixed_bytes};

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

struct MyBox([u8; 20]);

impl AsRef<[u8; 20]> for MyBox {
    fn as_ref(&self) -> &[u8; 20] {
        &self.0
    }
}

impl crate::hex::FromHex for MyBox {
    type Error = faster_hex::Error;
    fn from_hex(hex_str: &str) -> Result<Self, Self::Error> {
        let mut bytes = [0u8; 20];
        faster_hex::hex_decode(hex_str.as_bytes(), &mut bytes)?;
        Ok(MyBox(bytes))
    }
}

impl From<[u8; 20]> for MyBox {
    fn from(value: [u8; 20]) -> Self {
        MyBox(value)
    }
}

serde_impl_ser_fixed_bytes!(MyBox, 20);
serde_impl_deser_fixed_bytes!(MyBox, 20);
