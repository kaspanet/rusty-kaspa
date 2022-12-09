use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum NetworkType {
    Mainnet,
    Testnet,
    Simnet,
    Devnet,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct Address;

pub type RpcUtxoAddress = Vec<u8>;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct UtxosChangedNotification {
    pub utxo_address: RpcUtxoAddress,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualSelectedParentChainChangedNotification;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FinalityConflictNotification;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct FinalityConflictResolvedNotification;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualSelectedParentBlueScoreChangedNotification;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct VirtualDaaScoreChangedNotification;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub struct PruningPointUTXOSetOverrideNotification;
