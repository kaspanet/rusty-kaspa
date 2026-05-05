pub use de::Deserialize;
pub use ser::Serialize;

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

mod de {
    pub trait Deserialize<'de, const N: usize>: Sized {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>;
    }

    impl<'de, T: crate::serde_bytes_fixed::Deserialize<'de, N>, const N: usize> Deserialize<'de, N> for Option<T> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct OptionalVisitor<T, const N: usize> {
                out: core::marker::PhantomData<T>,
            }

            impl<'de, T, const N: usize> serde::de::Visitor<'de> for OptionalVisitor<T, N>
            where
                T: crate::serde_bytes_fixed::Deserialize<'de, N>,
            {
                type Value = Option<T>;

                fn expecting(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                    write!(f, "optional fixed-size byte array of size {N}")
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
                    <T as crate::serde_bytes_fixed::Deserialize<'de, N>>::deserialize(deserializer).map(Some)
                }
            }

            let visitor = OptionalVisitor::<T, N> { out: core::marker::PhantomData };
            deserializer.deserialize_option(visitor)
        }
    }
}

mod ser {
    use serde::Serializer;

    pub trait Serialize<const N: usize> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer;
    }

    impl<T, const N: usize> Serialize<N> for Option<T>
    where
        T: crate::serde_bytes_fixed_ref::Serialize<N>,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            struct AsBytes<'a, T, const N: usize>(&'a T);

            impl<T, const N: usize> serde::Serialize for AsBytes<'_, T, N>
            where
                T: crate::serde_bytes_fixed_ref::Serialize<N>,
            {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    crate::serde_bytes_fixed_ref::Serialize::serialize(self.0, serializer)
                }
            }

            match self {
                Some(b) => serializer.serialize_some(&AsBytes::<_, N>(b)),
                None => serializer.serialize_none(),
            }
        }
    }
}
