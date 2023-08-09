use serde::Serializer;
use std::str::{self};
use serde::ser::SerializeTuple;

pub trait Serialize<const N: usize> {
    #[allow(missing_docs)]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
}

impl<const N: usize, T: AsRef<[u8; N]>> Serialize<N> for T {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let mut hex = vec![0u8; self.as_ref().len() * 2];
            faster_hex::hex_encode(self.as_ref(), &mut hex[..]).map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(unsafe { str::from_utf8_unchecked(&hex) })
        } else {
            let t = serializer.serialize_tuple(self.as_ref().len())?;

            t.serialize_element()
            serializer.serialize_bytes(self.as_ref())
        }
    }
}
