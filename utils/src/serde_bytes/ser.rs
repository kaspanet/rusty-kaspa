use serde::Serializer;
use std::str::{self};

pub trait Serialize {
    #[allow(missing_docs)]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer;
}

impl<T: AsRef<[u8]>> Serialize for T {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let mut hex = vec![0u8; self.as_ref().len() * 2];
            faster_hex::hex_encode(self.as_ref(), &mut hex[..]).map_err(serde::ser::Error::custom)?;
            serializer.serialize_str(unsafe { str::from_utf8_unchecked(&hex) })
        } else {
            serializer.serialize_bytes(self.as_ref())
        }
    }
}
