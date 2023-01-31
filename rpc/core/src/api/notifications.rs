use crate::NotificationMessage;
use crate::{api::ops::RpcApiOps, model::message::*, RpcAddress};
use async_channel::{Receiver, Sender};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::sync::Arc;

#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum NotificationType {
    BlockAdded,
    FinalityConflict,
    FinalityConflictResolved,
    NewBlockTemplate,
    PruningPointUtxoSetOverride,
    UtxosChanged(Vec<RpcAddress>),
    VirtualDaaScoreChanged,
    VirtualSelectedParentBlueScoreChanged,
    VirtualSelectedParentChainChanged,
}

impl From<&Notification> for NotificationType {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => NotificationType::BlockAdded,
            Notification::FinalityConflict(_) => NotificationType::FinalityConflict,
            Notification::FinalityConflictResolved(_) => NotificationType::FinalityConflictResolved,
            Notification::NewBlockTemplate(_) => NotificationType::NewBlockTemplate,
            Notification::PruningPointUtxoSetOverride(_) => NotificationType::PruningPointUtxoSetOverride,
            Notification::UtxosChanged(_) => NotificationType::UtxosChanged(vec![]),
            Notification::VirtualDaaScoreChanged(_) => NotificationType::VirtualDaaScoreChanged,
            Notification::VirtualSelectedParentBlueScoreChanged(_) => NotificationType::VirtualSelectedParentBlueScoreChanged,
            Notification::VirtualSelectedParentChainChanged(_) => NotificationType::VirtualSelectedParentChainChanged,
        }
    }
}

impl From<&Notification> for RpcApiOps {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => RpcApiOps::NotifyBlockAdded,
            Notification::FinalityConflict(_) => RpcApiOps::NotifyFinalityConflict,
            Notification::FinalityConflictResolved(_) => RpcApiOps::NotifyFinalityConflictResolved,
            Notification::NewBlockTemplate(_) => RpcApiOps::NotifyNewBlockTemplate,
            Notification::PruningPointUtxoSetOverride(_) => RpcApiOps::NotifyPruningPointUtxoSetOverride,
            Notification::UtxosChanged(_) => RpcApiOps::NotifyUtxosChanged,
            Notification::VirtualDaaScoreChanged(_) => RpcApiOps::NotifyVirtualDaaScoreChanged,
            Notification::VirtualSelectedParentBlueScoreChanged(_) => RpcApiOps::NotifyVirtualSelectedParentBlueScoreChanged,
            Notification::VirtualSelectedParentChainChanged(_) => RpcApiOps::NotifyVirtualSelectedParentChainChanged,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
#[allow(clippy::large_enum_variant)]
pub enum Notification {
    BlockAdded(BlockAddedNotification),
    FinalityConflict(FinalityConflictNotification),
    FinalityConflictResolved(FinalityConflictResolvedNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),
    UtxosChanged(UtxosChangedNotification),
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),
    VirtualSelectedParentBlueScoreChanged(VirtualSelectedParentBlueScoreChangedNotification),
    VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedNotification),
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

pub type NotificationSender = Sender<Arc<NotificationMessage>>;
pub type NotificationReceiver = Receiver<Arc<NotificationMessage>>;

pub enum NotificationHandle {
    Existing(u64),
    New(NotificationSender),
}

impl AsRef<Notification> for Notification {
    fn as_ref(&self) -> &Self {
        self
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_notification_from_bytes() {
        let bytes = &vec![6, 169, 167, 75, 2, 0, 0, 0, 0][..];
        let notification = Notification::try_from_slice(bytes);
        println!("notification: {notification:?}");
    }
}
