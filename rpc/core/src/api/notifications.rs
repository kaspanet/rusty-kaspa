use crate::model::message::*;
use async_channel::{Receiver, Sender};
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_notify::{
    events::EventType,
    notification::Notification as NotificationTrait,
    scope::{Scope, UtxosChangedScope, VirtualSelectedParentChainChangedScope},
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualSelectedParentChainChangedSubscription},
        Single,
    },
};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::sync::Arc;

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[allow(clippy::large_enum_variant)]
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

impl AsRef<Notification> for Notification {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl NotificationTrait for Notification {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_selected_parent_chain_changed_subscription(
        &self,
        subscription: &VirtualSelectedParentChainChangedSubscription,
    ) -> Option<Self> {
        match subscription.active() {
            true => {
                if let Notification::VirtualSelectedParentChainChanged(ref payload) = self {
                    if !subscription.include_accepted_transaction_ids() && !payload.accepted_transaction_ids.is_empty() {
                        return Some(Notification::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedNotification {
                            removed_chain_block_hashes: payload.removed_chain_block_hashes.clone(),
                            added_chain_block_hashes: payload.added_chain_block_hashes.clone(),
                            accepted_transaction_ids: Arc::new(vec![]),
                        }));
                    }
                }
                Some(self.clone())
            }
            false => None,
        }
    }

    fn apply_utxos_changed_subscription(&self, _subscription: &UtxosChangedSubscription) -> Option<Self> {
        todo!()
    }

    fn event_type(&self) -> EventType {
        self.into()
    }
}

// TODO: write a macro to get this
impl From<&Notification> for EventType {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => EventType::BlockAdded,
            Notification::VirtualSelectedParentChainChanged(_) => EventType::VirtualSelectedParentChainChanged,
            Notification::FinalityConflict(_) => EventType::FinalityConflict,
            Notification::FinalityConflictResolved(_) => EventType::FinalityConflictResolved,
            Notification::UtxosChanged(_) => EventType::UtxosChanged,
            Notification::VirtualSelectedParentBlueScoreChanged(_) => EventType::VirtualSelectedParentBlueScoreChanged,
            Notification::VirtualDaaScoreChanged(_) => EventType::VirtualDaaScoreChanged,
            Notification::PruningPointUtxoSetOverride(_) => EventType::PruningPointUTXOSetOverride,
            Notification::NewBlockTemplate(_) => EventType::NewBlockTemplate,
        }
    }
}

impl From<&Notification> for Scope {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => Scope::BlockAdded,
            Notification::VirtualSelectedParentChainChanged(_) => {
                Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::default())
            }
            Notification::FinalityConflict(_) => Scope::FinalityConflict,
            Notification::FinalityConflictResolved(_) => Scope::FinalityConflictResolved,
            Notification::UtxosChanged(_) => Scope::UtxosChanged(UtxosChangedScope::default()),
            Notification::VirtualSelectedParentBlueScoreChanged(_) => Scope::VirtualSelectedParentBlueScoreChanged,
            Notification::VirtualDaaScoreChanged(_) => Scope::VirtualDaaScoreChanged,
            Notification::PruningPointUtxoSetOverride(_) => Scope::PruningPointUtxoSetOverride,
            Notification::NewBlockTemplate(_) => Scope::NewBlockTemplate,
        }
    }
}

pub type NotificationSender = Sender<Notification>;
pub type NotificationReceiver = Receiver<Notification>;

pub enum NotificationHandle {
    Existing(u64),
    New(NotificationSender),
}
