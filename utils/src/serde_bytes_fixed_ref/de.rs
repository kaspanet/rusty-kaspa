#[macro_export]
/// Macro to provide serde::Deserialize implementations for types `$t`
/// which can be constructed from byte arrays of fixed size.
/// The resulting structure will support deserialization from human-readable
/// formats using hex::FromHex, as well as binary formats.
macro_rules! serde_impl_deser_fixed_bytes_ref {
    ($t: ty, $size: expr) => {
        impl<'de> serde::Deserialize<'de> for $t {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct MyVisitor<'de> {
                    marker: std::marker::PhantomData<$t>,
                    lifetime: std::marker::PhantomData<&'de ()>,
                }
                impl<'de> serde::de::Visitor<'de> for MyVisitor<'de> {
                    type Value = $t;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(formatter, "a byte array of size {} to ", $size)?;
                        write!(formatter, "{}", stringify!($t))
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
                    deserializer.deserialize_newtype_struct(
                        stringify!($i),
                        MyVisitor { marker: Default::default(), lifetime: Default::default() },
                    )
                }
            }
        }
    };
}
