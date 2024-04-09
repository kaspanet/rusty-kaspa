use super::events::EventType;
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct UtxosChangedScope {
    pub addresses: Vec<Address>,
}

impl std::fmt::Display for UtxosChangedScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let addresses = match self.addresses.len() {
            0 => "all".to_string(),
            1 => format!("{}", self.addresses[0]),
            n => format!("{} addresses", n),
        };
        write!(f, "UtxosChangedScope ({})", addresses)
    }
}

impl PartialEq for UtxosChangedScope {
    fn eq(&self, other: &Self) -> bool {
        self.addresses.len() == other.addresses.len() && self.addresses.iter().all(|x| other.addresses.contains(x))
    }
}

impl Eq for UtxosChangedScope {}

impl UtxosChangedScope {
    pub fn new(addresses: Vec<Address>) -> Self {
        Self { addresses }
    }
}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct SinkBlueScoreChangedScope {}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct VirtualDaaScoreChangedScope {}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct PruningPointUtxoSetOverrideScope {}

#[derive(Clone, Display, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct NewBlockTemplateScope {}
