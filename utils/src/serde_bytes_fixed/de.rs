/// Trait for deserialization of fixed-size byte arrays, newtype structs,
/// or any other custom types that can implement `From<[u8; N]>`.
/// Implementers should also provide implementation for `crate::hex::FromHex`.
/// N must be in range: (0..=32), this is serde limitation
pub trait Deserialize<'de, const N: usize>: Sized + crate::hex::FromHex + From<[u8; N]> {
    /// Deserialize given `deserializer` into `Self`.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>;
}

/// Macro to generate impl of `Deserialize` for types `T`, which are
/// capable of being constructed from byte arrays of fixed size,
/// or newtype structs or other types based on [`From<[u8; $size]>`].
macro_rules! deser_fixed_bytes {
    ($size: expr) => {
        impl<'de, T: $crate::hex::FromHex + From<[u8; $size]>> $crate::serde_bytes_fixed::Deserialize<'de, $size> for T {
            /// Deserialization function for types `T`
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct FixedBytesVisitor<'de> {
                    marker: std::marker::PhantomData<[u8; $size]>,
                    lifetime: std::marker::PhantomData<&'de ()>,
                }
                impl<'de> serde::de::Visitor<'de> for FixedBytesVisitor<'de> {
                    type Value = [u8; $size];

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(formatter, "a byte array of size {}", $size)
                    }
                    #[inline]
                    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        let v: [u8; $size] = v.as_bytes().try_into().map_err(serde::de::Error::custom)?;
                        Ok(Self::Value::from(v))
                    }
                    #[inline]
                    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        let v: [u8; $size] = v.as_bytes().try_into().map_err(serde::de::Error::custom)?;
                        Ok(Self::Value::from(v))
                    }
                    #[inline]
                    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        let v: [u8; $size] = v.try_into().map_err(serde::de::Error::custom)?;
                        Ok(Self::Value::from(v))
                    }
                    #[inline]
                    fn visit_borrowed_bytes<E>(self, v: &'de [u8]) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        let v: [u8; $size] = v.try_into().map_err(serde::de::Error::custom)?;
                        Ok(Self::Value::from(v))
                    }

                    #[inline]
                    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
                    where
                        D: serde::Deserializer<'de>,
                    {
                        <[u8; $size] as serde::Deserialize>::deserialize(deserializer).map(|v| Self::Value::from(v))
                    }
                    #[inline]
                    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                    where
                        A: serde::de::SeqAccess<'de>,
                    {
                        let Some(value): Option<[u8; $size]> = seq.next_element()? else {
                            return Err(serde::de::Error::invalid_length(0usize, &"tuple struct fixed array with 1 element"));
                        };
                        Ok(Self::Value::from(value))
                    }
                }

                if deserializer.is_human_readable() {
                    deserializer.deserialize_str($crate::serde_bytes::FromHexVisitor::default())
                } else {
                    deserializer
                        .deserialize_tuple($size, FixedBytesVisitor { marker: Default::default(), lifetime: Default::default() })
                        .map(Into::into)
                }
            }
        }
    };
}

macro_rules! apply_deser_fixed_bytes {
    ($($x:expr),*) => ($(deser_fixed_bytes!($x);)*)
}

apply_deser_fixed_bytes!(
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32
);
