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

/// Implementation of Serialize trait for types that can be referenced as fixed-size byte array.
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
            let mut t = serializer.serialize_tuple(self.as_ref().len())?;
            for v in self.as_ref() {
                t.serialize_element(v)?;
            }
            t.end()
        }
    }
}

#[macro_export]
/// Macro to generate serde::Serialize implementation for types `$t` that can be referenced as a byte array of fixed size,
/// The resulting structure will support serialization into human-readable formats using hex encoding,
/// as well as binary formats.
macro_rules! serde_impl_ser_fixed_bytes_ref {
    ($t: ty, $size: expr) => {
        impl serde::Serialize for $t {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                let len = std::convert::AsRef::<[u8; $size]>::as_ref(self).len();
                if serializer.is_human_readable() {
                    let mut hex = vec![0u8; len * 2];
                    faster_hex::hex_encode(&std::convert::AsRef::<[u8; $size]>::as_ref(self)[..], &mut hex[..])
                        .map_err(serde::ser::Error::custom)?;
                    serializer.serialize_str(unsafe { std::str::from_utf8_unchecked(&hex) })
                } else {
                    let mut t = serializer.serialize_tuple(len)?;
                    for v in std::convert::AsRef::<[u8; $size]>::as_ref(self) {
                        serde::ser::SerializeTuple::serialize_element(&mut t, v)?
                    }
                    serde::ser::SerializeTuple::end(t)
                }
            }
        }
    };
}
