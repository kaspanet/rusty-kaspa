use crate::model::message::*;
use crate::{stubs::*, RpcAddress};
use addresses::Address;
use async_channel::{Receiver, Sender};
use borsh::{BorshDeserialize, BorshSchema, BorshSerialize};
use consensus_core::notify::ConsensusNotification;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use ahash::AHashSet;
use std::sync::Arc;
use into_variant::VariantFrom;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, VariantFrom)]
pub enum NotificationType {
    BlockAdded,
    VirtualSelectedParentChainChanged,
    FinalityConflicts,
    FinalityConflictResolved,
    UtxosChanged(UtxoChangedNotificationTypeModification),
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUTXOSetOverride,
    NewBlockTemplate,
    VirtualChangeSet,
}

impl From<&Notification> for NotificationType {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => NotificationType::BlockAdded,
            Notification::VirtualSelectedParentChainChanged(_) => NotificationType::VirtualSelectedParentChainChanged,
            Notification::FinalityConflict(_) => NotificationType::FinalityConflicts,
            Notification::FinalityConflictResolved(_) => NotificationType::FinalityConflictResolved,
            Notification::UtxosChanged(_) => NotificationType::UtxosChanged(UtxosChangedNotificationTypeModification::default()),
            Notification::VirtualSelectedParentBlueScoreChanged(_) => NotificationType::VirtualSelectedParentBlueScoreChanged,
            Notification::VirtualDaaScoreChanged(_) => NotificationType::VirtualDaaScoreChanged,
            Notification::PruningPointUTXOSetOverride(_) => NotificationType::PruningPointUTXOSetOverride,
            Notification::NewBlockTemplate(_) => NotificationType::NewBlockTemplate,
            Notification::VirtualChangeSet(_) => NotificationType::VirtualChangeSet,
            _ => panic!("Notification: {:?}, is not a notificationType", item),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize, BorshSchema, VariantFrom)]
#[allow(clippy::large_enum_variant)]
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

pub type NotificationSender = Sender<Arc<Notification>>;
pub type NotificationReceiver = Receiver<Arc<Notification>>;

pub enum NotificationHandle {
    Existing(u64),
    New(NotificationSender),
}

impl AsRef<Notification> for Notification {
    fn as_ref(&self) -> &Self {
        self
    }
}
