use crate::model::message::*;
use crate::stubs::*;
use async_channel::{Receiver, Sender};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema)]
pub enum NotificationType {
    BlockAdded,
    VirtualSelectedParentChainChanged,
    FinalityConflicts,
    FinalityConflictResolved,
    UtxosChanged(Vec<RpcUtxoAddress>),
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUTXOSetOverride,
    NewBlockTemplate,
}

impl From<&Notification> for NotificationType {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => NotificationType::BlockAdded,
            Notification::VirtualSelectedParentChainChanged(_) => NotificationType::VirtualSelectedParentChainChanged,
            Notification::FinalityConflict(_) => NotificationType::FinalityConflicts,
            Notification::FinalityConflictResolved(_) => NotificationType::FinalityConflictResolved,
            Notification::UtxosChanged(_) => NotificationType::UtxosChanged(vec![]),
            Notification::VirtualSelectedParentBlueScoreChanged(_) => NotificationType::VirtualSelectedParentBlueScoreChanged,
            Notification::VirtualDaaScoreChanged(_) => NotificationType::VirtualDaaScoreChanged,
            Notification::PruningPointUTXOSetOverride(_) => NotificationType::PruningPointUTXOSetOverride,
            Notification::NewBlockTemplate(_) => NotificationType::NewBlockTemplate,
        }
    }
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
    PruningPointUTXOSetOverride(PruningPointUTXOSetOverrideNotification),
    NewBlockTemplate(NewBlockTemplateNotification),
}

impl Display for Notification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Notification::BlockAdded(ref notification) => {
                write!(f, "BlockAdded notification with hash {}", notification.block.header.hash)
            }
            Notification::NewBlockTemplate(_) => {
                write!(f, "NewBlockTemplate notification")
            }
            _ => write!(f, "Notification type not implemented yet"),
            // Notification::VirtualSelectedParentChainChanged(_) => todo!(),
            // Notification::FinalityConflict(_) => todo!(),
            // Notification::FinalityConflictResolved(_) => todo!(),
            // Notification::UtxosChanged(_) => todo!(),
            // Notification::VirtualSelectedParentBlueScoreChanged(_) => todo!(),
            // Notification::VirtualDaaScoreChanged(_) => todo!(),
            // Notification::PruningPointUTXOSetOverride(_) => todo!(),
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
