use crate::{model::message::*, RpcAddress};
use async_channel::{Receiver, Sender};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum NotificationType {
    BlockAdded,
    VirtualSelectedParentChainChanged,
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged(Vec<RpcAddress>),
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
    NewBlockTemplate,
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[allow(clippy::large_enum_variant)] //TODO: solution: use targeted Arcs on large inner notifications to reduce size
pub enum Notification {
    BlockAdded(BlockAddedNotification),
    VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedNotification),
    FinalityConflict(FinalityConflictNotification),
    FinalityConflictResolved(FinalityConflictResolvedNotification),
    UtxosChanged(UtxosChangedNotification),
    VirtualSelectedParentBlueScoreChanged(VirtualSelectedParentBlueScoreChangedNotification),
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
}

impl Display for Notification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Notification::BlockAdded(ref notification) => {
                write!(f, "BlockAdded notification: block hash {}", notification.block.header.hash)
            }
            Notification::NewBlockTemplate(_) => {
                write!(f, "NewBlockTemplate notification")
            }
            Notification::VirtualSelectedParentChainChanged(ref notification) => {
                write!(
                    f,
                    "VirtualSelectedParentChainChanged notification: {} removed blocks, {} added blocks, {} accepted transactions",
                    notification.removed_chain_block_hashes.len(),
                    notification.added_chain_block_hashes.len(),
                    notification.accepted_transaction_ids.len()
                )
            }
            Notification::FinalityConflict(ref notification) => {
                write!(f, "FinalityConflict notification: violating block hash {}", notification.violating_block_hash)
            }
            Notification::FinalityConflictResolved(ref notification) => {
                write!(f, "FinalityConflictResolved notification: finality block hash {}", notification.finality_block_hash)
            }
            Notification::UtxosChanged(ref notification) => {
                write!(f, "UtxosChanged notification: {} removed, {} added", notification.removed.len(), notification.added.len())
            }
            Notification::VirtualSelectedParentBlueScoreChanged(ref notification) => {
                write!(
                    f,
                    "VirtualSelectedParentBlueScoreChanged notification: virtual selected parent blue score {}",
                    notification.virtual_selected_parent_blue_score
                )
            }
            Notification::VirtualDaaScoreChanged(ref notification) => {
                write!(f, "VirtualDaaScoreChanged notification: virtual DAA score {}", notification.virtual_daa_score)
            }
            Notification::PruningPointUtxoSetOverride(_) => {
                write!(f, "PruningPointUtxoSetOverride notification")
            }
        }
    }
}

pub type NotificationSender = Sender<Notification>;
pub type NotificationReceiver = Receiver<Notification>;

pub enum NotificationHandle {
    Existing(u64),
    New(NotificationSender),
}

impl AsRef<Notification> for Notification {
    fn as_ref(&self) -> &Self {
        self
    }
}
