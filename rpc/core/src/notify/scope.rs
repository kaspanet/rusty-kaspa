use addresses::Address;
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum Scope {
    BlockAdded,
    VirtualSelectedParentChainChanged(bool),
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged(Vec<Address>),
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
    NewBlockTemplate,
}
