pub use de::Deserialize;
pub use ser::Serialize;

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

mod de {
    use std::fmt::Display;

    pub trait Deserialize<'de>: Sized {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>;
    }

    impl<'de, T: crate::serde_bytes::Deserialize<'de>> Deserialize<'de> for Option<T>
    where
        <T as TryFrom<&'de [u8]>>::Error: Display,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct OptionalVisitor<T> {
                out: std::marker::PhantomData<T>,
            }

            impl<'de, T> serde::de::Visitor<'de> for OptionalVisitor<T>
            where
                T: crate::serde_bytes::Deserialize<'de>,
            {
                type Value = Option<T>;

                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("optional string, str or slice, vec of bytes")
                }

                fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                    Ok(None)
                }

                fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                    Ok(None)
                }

                fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
                where
                    D: serde::Deserializer<'de>,
                {
                    T::deserialize(deserializer).map(Some)
                }
            }

            let visitor = OptionalVisitor { out: std::marker::PhantomData };
            deserializer.deserialize_option(visitor)
        }
    }
}

mod ser {
    use serde::Serializer;

    pub trait Serialize {
        #[allow(missing_docs)]
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer;
    }

    impl<T> Serialize for Option<T>
    where
        T: crate::serde_bytes::Serialize + std::convert::AsRef<[u8]>,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            struct AsBytes<T>(T);

            impl<T> serde::Serialize for AsBytes<T>
            where
                T: crate::serde_bytes::Serialize,
            {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    crate::serde_bytes::Serialize::serialize(&self.0, serializer)
                }
            }

            match self {
                Some(b) => serializer.serialize_some(&AsBytes(b)),
                None => serializer.serialize_none(),
            }
        }
    }
}
