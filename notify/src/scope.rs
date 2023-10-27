use super::events::EventType;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct BlockAddedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualChainChangedScope {
    pub include_accepted_transaction_ids: bool,
}

impl VirtualChainChangedScope {
    pub fn new(include_accepted_transaction_ids: bool) -> Self {
        Self { include_accepted_transaction_ids }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FinalityConflictScope {}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FinalityConflictResolvedScope {}

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct UtxosChangedScope {
    pub addresses: Vec<Address>,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct SinkBlueScoreChangedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualDaaScoreChangedScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct PruningPointUtxoSetOverrideScope {}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct NewBlockTemplateScope {}
