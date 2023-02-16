use super::events::EventType;
use addresses::Address;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Scope {
    BlockAdded,
    VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope),
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged(UtxosChangedScope),
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
    NewBlockTemplate,
}

// TODO: write a macro to get this
impl From<EventType> for Scope {
    fn from(item: EventType) -> Self {
        match item {
            EventType::BlockAdded => Scope::BlockAdded,
            EventType::VirtualSelectedParentChainChanged => {
                Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::default())
            }
            EventType::FinalityConflict => Scope::FinalityConflict,
            EventType::FinalityConflictResolved => Scope::FinalityConflictResolved,
            EventType::UtxosChanged => Scope::UtxosChanged(UtxosChangedScope::default()),
            EventType::VirtualSelectedParentBlueScoreChanged => Scope::VirtualSelectedParentBlueScoreChanged,
            EventType::VirtualDaaScoreChanged => Scope::VirtualDaaScoreChanged,
            EventType::PruningPointUTXOSetOverride => Scope::PruningPointUtxoSetOverride,
            EventType::NewBlockTemplate => Scope::NewBlockTemplate,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualSelectedParentChainChangedScope {
    pub include_accepted_transaction_ids: bool,
}

impl VirtualSelectedParentChainChangedScope {
    pub fn new(include_accepted_transaction_ids: bool) -> Self {
        Self { include_accepted_transaction_ids }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct UtxosChangedScope {
    pub addresses: Vec<Address>,
}

impl UtxosChangedScope {
    pub fn new(addresses: Vec<Address>) -> Self {
        Self { addresses }
    }
}
