use crate::{
    address::tracker::{Indexer, Indexes},
    events::EventType,
};
use borsh::{BorshDeserialize, BorshSerialize};
use derive_more::Display;
use kaspa_addresses::Address;
use serde::{Deserialize, Serialize};

macro_rules! scope_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
    $($(#[$variant_meta:meta])* $variant_name:ident,)*
    }) => {
        paste::paste! {
            $(#[$meta])*
            $vis enum $name {
                $($(#[$variant_meta])* $variant_name([<$variant_name Scope>])),*
            }

            impl std::convert::From<EventType> for $name {
                fn from(value: EventType) -> Self {
                    match value {
                        $(EventType::$variant_name => $name::$variant_name(kaspa_notify::scope::[<$variant_name Scope>]::default())),*
                    }
                }
            }

            $(impl std::convert::From<[<$variant_name Scope>]> for Scope {
                fn from(value: [<$variant_name Scope>]) -> Self {
                    Scope::$variant_name(value)
                }
            })*
        }
    }
}

scope_enum! {
/// Subscription scope for every event type
#[derive(Clone, Display, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Scope {
    BlockAdded,
    VirtualChainChanged,
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged,
    SinkBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
    NewBlockTemplate,
}
}

impl Scope {
    pub fn event_type(&self) -> EventType {
        self.into()
    }
}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct BlockAddedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VirtualChainChangedScope {
    pub include_accepted_transaction_ids: bool,
}

impl VirtualChainChangedScope {
    pub fn new(include_accepted_transaction_ids: bool) -> Self {
        Self { include_accepted_transaction_ids }
    }
}

impl std::fmt::Display for VirtualChainChangedScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VirtualChainChangedScope{}", if self.include_accepted_transaction_ids { " with accepted transactions" } else { "" })
    }
}

#[derive(Clone, Display, Debug, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct FinalityConflictScope {}

#[derive(Clone, Display, Debug, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct FinalityConflictResolvedScope {}

#[derive(Clone, Debug)]
pub enum UtxosChangedScope {
    Addresses(Vec<Address>),
    Indexes(Indexes),
}

impl UtxosChangedScope {
    pub fn new(addresses: Vec<Address>) -> Self {
        Self::Addresses(addresses)
    }

    pub fn new_indexes(indexes: Indexes) -> Self {
        Self::Indexes(indexes)
    }

    pub fn is_empty(&self) -> bool {
        match self {
            Self::Addresses(ref addresses) => addresses.is_empty(),
            Self::Indexes(ref indexes) => indexes.is_empty(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Addresses(ref addresses) => addresses.len(),
            Self::Indexes(ref indexes) => indexes.len(),
        }
    }
}

impl Default for UtxosChangedScope {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl std::fmt::Display for UtxosChangedScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let addresses = match self {
            Self::Addresses(ref addresses) => match addresses.len() {
                0 => "all".to_string(),
                1 => format!("{}", addresses[0]),
                n => format!("{} addresses", n),
            },
            Self::Indexes(ref indexes) => match indexes.len() {
                0 => "all".to_string(),
                n => format!("{} addresses", n),
            },
        };
        write!(f, "UtxosChangedScope ({})", addresses)
    }
}

impl PartialEq for UtxosChangedScope {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Addresses(ref addresses) => match other {
                Self::Addresses(ref other_addresses) => {
                    addresses.len() == other_addresses.len() && addresses.iter().all(|x| other_addresses.contains(x))
                }
                Self::Indexes(ref other_indexes) => {
                    addresses.len() == other_indexes.len() && addresses.iter().all(|x| other_indexes.contains(x))
                }
            },
            Self::Indexes(ref indexes) => match other {
                Self::Addresses(ref other_addresses) => {
                    indexes.len() == other_addresses.len() && other_addresses.iter().all(|x| indexes.contains(x))
                }
                Self::Indexes(ref other_indexes) => {
                    indexes.len() == other_indexes.len() && indexes.iter_index().all(|x| other_indexes.contains_index(x))
                }
            },
        }
    }
}

impl Eq for UtxosChangedScope {}

impl BorshSerialize for UtxosChangedScope
where
    Vec<Address>: BorshSerialize,
{
    fn serialize<W: borsh::maybestd::io::Write>(&self, writer: &mut W) -> ::core::result::Result<(), borsh::maybestd::io::Error> {
        match self {
            Self::Addresses(ref addresses) => {
                borsh::BorshSerialize::serialize(addresses, writer)?;
            }
            Self::Indexes(_) => {
                todo!("Convert indexes into addresses and serialize");
            }
        }
        Ok(())
    }
}

impl BorshDeserialize for UtxosChangedScope
where
    Vec<Address>: BorshDeserialize,
{
    fn deserialize(buf: &mut &[u8]) -> ::core::result::Result<Self, borsh::maybestd::io::Error> {
        let addresses = borsh::BorshDeserialize::deserialize(buf)?;
        Ok(Self::new(addresses))
    }
}

#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl _serde::Serialize for UtxosChangedScope {
        fn serialize<__S>(&self, __serializer: __S) -> _serde::__private::Result<__S::Ok, __S::Error>
        where
            __S: _serde::Serializer,
        {
            match self {
                Self::Addresses(ref addresses) => {
                    let mut __serde_state =
                        _serde::Serializer::serialize_struct(__serializer, "UtxosChangedScope", false as usize + 1)?;
                    _serde::ser::SerializeStruct::serialize_field(&mut __serde_state, "addresses", addresses)?;
                    _serde::ser::SerializeStruct::end(__serde_state)
                }
                Self::Indexes(_) => {
                    todo!("Convert indexes into addresses and serialize");
                }
            }
        }
    }
};

#[doc(hidden)]
#[allow(non_upper_case_globals, unused_attributes, unused_qualifications)]
const _: () = {
    #[allow(unused_extern_crates, clippy::useless_attribute)]
    extern crate serde as _serde;
    #[automatically_derived]
    impl<'de> _serde::Deserialize<'de> for UtxosChangedScope {
        fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
        where
            __D: _serde::Deserializer<'de>,
        {
            #[allow(non_camel_case_types)]
            #[doc(hidden)]
            enum __Field {
                __field0,
                __ignore,
            }
            #[doc(hidden)]
            struct __FieldVisitor;

            impl<'de> _serde::de::Visitor<'de> for __FieldVisitor {
                type Value = __Field;
                fn expecting(&self, __formatter: &mut _serde::__private::Formatter) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "field identifier")
                }
                fn visit_u64<__E>(self, __value: u64) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        0u64 => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_str<__E>(self, __value: &str) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        "addresses" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
                fn visit_bytes<__E>(self, __value: &[u8]) -> _serde::__private::Result<Self::Value, __E>
                where
                    __E: _serde::de::Error,
                {
                    match __value {
                        b"addresses" => _serde::__private::Ok(__Field::__field0),
                        _ => _serde::__private::Ok(__Field::__ignore),
                    }
                }
            }
            impl<'de> _serde::Deserialize<'de> for __Field {
                #[inline]
                fn deserialize<__D>(__deserializer: __D) -> _serde::__private::Result<Self, __D::Error>
                where
                    __D: _serde::Deserializer<'de>,
                {
                    _serde::Deserializer::deserialize_identifier(__deserializer, __FieldVisitor)
                }
            }
            #[doc(hidden)]
            struct __Visitor<'de> {
                marker: _serde::__private::PhantomData<UtxosChangedScope>,
                lifetime: _serde::__private::PhantomData<&'de ()>,
            }
            impl<'de> _serde::de::Visitor<'de> for __Visitor<'de> {
                type Value = UtxosChangedScope;
                fn expecting(&self, __formatter: &mut _serde::__private::Formatter) -> _serde::__private::fmt::Result {
                    _serde::__private::Formatter::write_str(__formatter, "struct UtxosChangedScope")
                }
                #[inline]
                fn visit_seq<__A>(self, mut __seq: __A) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::SeqAccess<'de>,
                {
                    let __field0 = match _serde::de::SeqAccess::next_element::<Vec<Address>>(&mut __seq)? {
                        _serde::__private::Some(__value) => __value,
                        _serde::__private::None => {
                            return _serde::__private::Err(_serde::de::Error::invalid_length(
                                0usize,
                                &"struct UtxosChangedScope with 1 element",
                            ))
                        }
                    };
                    _serde::__private::Ok(UtxosChangedScope::new(__field0))
                }
                #[inline]
                fn visit_map<__A>(self, mut __map: __A) -> _serde::__private::Result<Self::Value, __A::Error>
                where
                    __A: _serde::de::MapAccess<'de>,
                {
                    let mut __field0: _serde::__private::Option<Vec<Address>> = _serde::__private::None;
                    while let _serde::__private::Some(__key) = _serde::de::MapAccess::next_key::<__Field>(&mut __map)? {
                        match __key {
                            __Field::__field0 => {
                                if _serde::__private::Option::is_some(&__field0) {
                                    return _serde::__private::Err(<__A::Error as _serde::de::Error>::duplicate_field("addresses"));
                                }
                                __field0 = _serde::__private::Some(_serde::de::MapAccess::next_value::<Vec<Address>>(&mut __map)?);
                            }
                            _ => {
                                let _ = _serde::de::MapAccess::next_value::<_serde::de::IgnoredAny>(&mut __map)?;
                            }
                        }
                    }
                    let __field0 = match __field0 {
                        _serde::__private::Some(__field0) => __field0,
                        _serde::__private::None => _serde::__private::de::missing_field("addresses")?,
                    };
                    _serde::__private::Ok(UtxosChangedScope::new(__field0))
                }
            }
            #[doc(hidden)]
            const FIELDS: &[&str] = &["addresses"];
            _serde::Deserializer::deserialize_struct(
                __deserializer,
                "UtxosChangedScope",
                FIELDS,
                __Visitor { marker: _serde::__private::PhantomData::<UtxosChangedScope>, lifetime: _serde::__private::PhantomData },
            )
        }
    }
};

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SinkBlueScoreChangedScope {}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VirtualDaaScoreChangedScope {}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PruningPointUtxoSetOverrideScope {}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct NewBlockTemplateScope {}
