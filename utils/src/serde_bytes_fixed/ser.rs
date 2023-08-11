use serde::ser::SerializeTuple;
use serde::Serializer;
use std::str;

/// Trait for serialization of types which can be referenced as fixed-size byte arrays.
pub trait Serialize<const N: usize> {
    /// Serialize `self` using the provided Serializer.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
}

impl<const N: usize> Serialize<N> for [u8; N] {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let mut hex = vec![0u8; self.len() * 2];
            faster_hex::hex_encode(self, &mut hex[..]).map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(unsafe { str::from_utf8_unchecked(&hex) })
        } else {
            let mut t = serializer.serialize_tuple(self.len())?;
            for v in self {
                t.serialize_element(v)?;
            }
            t.end()
        }
    }
}
