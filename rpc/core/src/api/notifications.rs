use crate::model::message::*;
use derive_more::Display;
use kaspa_notify::{
    events::EventType,
    notification::{full_featured, Notification as NotificationTrait},
    subscription::{
        context::SubscriptionContext,
        single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
        Subscription,
    },
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use wasm_bindgen::JsValue;
use workflow_serializer::prelude::*;
use workflow_wasm::serde::to_value;

full_featured! {
#[derive(Clone, Debug, Display, Serialize, Deserialize)]
pub enum Notification {
    #[display(fmt = "BlockAdded notification: block hash {}", "_0.block.header.hash")]
    BlockAdded(BlockAddedNotification),

    #[display(fmt = "VirtualChainChanged notification: {} removed blocks, {} added blocks, {} accepted transactions", "_0.removed_chain_block_hashes.len()", "_0.added_chain_block_hashes.len()", "_0.accepted_transaction_ids.len()")]
    VirtualChainChanged(VirtualChainChangedNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.violating_block_hash")]
    FinalityConflict(FinalityConflictNotification),

    #[display(fmt = "FinalityConflict notification: violating block hash {}", "_0.finality_block_hash")]
    FinalityConflictResolved(FinalityConflictResolvedNotification),

    #[display(fmt = "UtxosChanged notification: {} removed, {} added", "_0.removed.len()", "_0.added.len()")]
    UtxosChanged(UtxosChangedNotification),

    #[display(fmt = "SinkBlueScoreChanged notification: virtual selected parent blue score {}", "_0.sink_blue_score")]
    SinkBlueScoreChanged(SinkBlueScoreChangedNotification),

    #[display(fmt = "VirtualDaaScoreChanged notification: virtual DAA score {}", "_0.virtual_daa_score")]
    VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification),

    #[display(fmt = "PruningPointUtxoSetOverride notification")]
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),

    #[display(fmt = "NewBlockTemplate notification")]
    NewBlockTemplate(NewBlockTemplateNotification),
}
}

impl Notification {
    ///
    pub fn to_value(&self) -> std::result::Result<JsValue, serde_wasm_bindgen::Error> {
        match self {
            Notification::BlockAdded(v) => to_value(&v),
            Notification::FinalityConflict(v) => to_value(&v),
            Notification::FinalityConflictResolved(v) => to_value(&v),
            Notification::NewBlockTemplate(v) => to_value(&v),
            Notification::PruningPointUtxoSetOverride(v) => to_value(&v),
            Notification::UtxosChanged(v) => to_value(&v),
            Notification::VirtualDaaScoreChanged(v) => to_value(&v),
            Notification::SinkBlueScoreChanged(v) => to_value(&v),
            Notification::VirtualChainChanged(v) => to_value(&v),
        }
    }
}

impl NotificationTrait for Notification {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription, _context: &SubscriptionContext) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_chain_changed_subscription(
        &self,
        subscription: &VirtualChainChangedSubscription,
        _context: &SubscriptionContext,
    ) -> Option<Self> {
        match subscription.active() {
            true => {
                if let Notification::VirtualChainChanged(ref payload) = self {
                    if !subscription.include_accepted_transaction_ids() && !payload.accepted_transaction_ids.is_empty() {
                        return Some(Notification::VirtualChainChanged(VirtualChainChangedNotification {
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

    fn apply_utxos_changed_subscription(
        &self,
        subscription: &UtxosChangedSubscription,
        context: &SubscriptionContext,
    ) -> Option<Self> {
        match subscription.active() {
            true => {
                let Self::UtxosChanged(notification) = self else { return None };
                notification.apply_utxos_changed_subscription(subscription, context).map(Self::UtxosChanged)
            }
            false => None,
        }
    }

    fn event_type(&self) -> EventType {
        self.into()
    }
}

impl Serializer for Notification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        match self {
            Notification::BlockAdded(notification) => {
                store!(u16, &0, writer)?;
                serialize!(BlockAddedNotification, notification, writer)?;
            }
            Notification::VirtualChainChanged(notification) => {
                store!(u16, &1, writer)?;
                serialize!(VirtualChainChangedNotification, notification, writer)?;
            }
            Notification::FinalityConflict(notification) => {
                store!(u16, &2, writer)?;
                serialize!(FinalityConflictNotification, notification, writer)?;
            }
            Notification::FinalityConflictResolved(notification) => {
                store!(u16, &3, writer)?;
                serialize!(FinalityConflictResolvedNotification, notification, writer)?;
            }
            Notification::UtxosChanged(notification) => {
                store!(u16, &4, writer)?;
                serialize!(UtxosChangedNotification, notification, writer)?;
            }
            Notification::SinkBlueScoreChanged(notification) => {
                store!(u16, &5, writer)?;
                serialize!(SinkBlueScoreChangedNotification, notification, writer)?;
            }
            Notification::VirtualDaaScoreChanged(notification) => {
                store!(u16, &6, writer)?;
                serialize!(VirtualDaaScoreChangedNotification, notification, writer)?;
            }
            Notification::PruningPointUtxoSetOverride(notification) => {
                store!(u16, &7, writer)?;
                serialize!(PruningPointUtxoSetOverrideNotification, notification, writer)?;
            }
            Notification::NewBlockTemplate(notification) => {
                store!(u16, &8, writer)?;
                serialize!(NewBlockTemplateNotification, notification, writer)?;
            }
        }
        Ok(())
    }
}

impl Deserializer for Notification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        match load!(u16, reader)? {
            0 => {
                let notification = deserialize!(BlockAddedNotification, reader)?;
                Ok(Notification::BlockAdded(notification))
            }
            1 => {
                let notification = deserialize!(VirtualChainChangedNotification, reader)?;
                Ok(Notification::VirtualChainChanged(notification))
            }
            2 => {
                let notification = deserialize!(FinalityConflictNotification, reader)?;
                Ok(Notification::FinalityConflict(notification))
            }
            3 => {
                let notification = deserialize!(FinalityConflictResolvedNotification, reader)?;
                Ok(Notification::FinalityConflictResolved(notification))
            }
            4 => {
                let notification = deserialize!(UtxosChangedNotification, reader)?;
                Ok(Notification::UtxosChanged(notification))
            }
            5 => {
                let notification = deserialize!(SinkBlueScoreChangedNotification, reader)?;
                Ok(Notification::SinkBlueScoreChanged(notification))
            }
            6 => {
                let notification = deserialize!(VirtualDaaScoreChangedNotification, reader)?;
                Ok(Notification::VirtualDaaScoreChanged(notification))
            }
            7 => {
                let notification = deserialize!(PruningPointUtxoSetOverrideNotification, reader)?;
                Ok(Notification::PruningPointUtxoSetOverride(notification))
            }
            8 => {
                let notification = deserialize!(NewBlockTemplateNotification, reader)?;
                Ok(Notification::NewBlockTemplate(notification))
            }
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid variant")),
        }
    }
}
