use crate::model::message::*;
use async_channel::{Receiver, Sender};
use borsh::{BorshDeserialize, BorshSerialize};
use derive_more::Display;
use kaspa_notify::{
    events::EventType,
    notification::{full_featured, Notification as NotificationTrait},
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualSelectedParentChainChangedSubscription},
        Single,
    },
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

full_featured! {
#[derive(Clone, Debug, Display, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Notification {
    #[display(fmt = "BlockAdded notification: block hash {}", "_0.block.header.hash")]
    BlockAdded(BlockAddedNotification),

    #[display(fmt = "VirtualSelectedParentChainChanged notification: {} removed blocks, {} added blocks, {} accepted transactions", "_0.removed_chain_block_hashes.len()", "_0.added_chain_block_hashes.len()", "_0.accepted_transaction_ids.len()")]
    VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.violating_block_hash")]
    FinalityConflict(FinalityConflictNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.finality_block_hash")]
    FinalityConflictResolved(FinalityConflictResolvedNotification),

    #[display(fmt = "UtxosChanged notification: {} removed, {} added", "_0.removed.len()", "_0.added.len()")]
    UtxosChanged(UtxosChangedNotification),

    #[display(fmt = "VirtualSelectedParentBlueScoreChanged notification: virtual selected parent blue score {}", "_0.virtual_selected_parent_blue_score")]
    VirtualSelectedParentBlueScoreChanged(VirtualSelectedParentBlueScoreChangedNotification),

    #[display(fmt = "VirtualDaaScoreChanged notification: virtual DAA score {}", "_0.virtual_daa_score")]
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),

    #[display(fmt = "PruningPointUtxoSetOverride notification")]
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),

    #[display(fmt = "NewBlockTemplate notification")]
    NewBlockTemplate(NewBlockTemplateNotification),
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

pub type NotificationSender = Sender<Notification>;
pub type NotificationReceiver = Receiver<Notification>;

pub enum NotificationHandle {
    Existing(u64),
    New(NotificationSender),
}
