use crate::indexed_utxos::{UtxoChanges, UtxoSetByScriptPublicKey};
use derive_more::Display;
use kaspa_addresses::Prefix;
use kaspa_notify::{
    events::EventType,
    full_featured,
    notification::Notification as NotificationTrait,
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
        Single,
    },
};
use std::sync::Arc;

full_featured! {
#[derive(Clone, Debug, Display)]
pub enum Notification {
    #[display(fmt = "UtxosChanged notification")]
    UtxosChanged(UtxosChangedNotification),

    #[display(fmt = "PruningPointUtxoSetOverride notification")]
    PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification),
}
}

impl NotificationTrait for Notification {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_chain_changed_subscription(&self, _subscription: &VirtualChainChangedSubscription) -> Option<Self> {
        Some(self.clone())
    }

    fn apply_utxos_changed_subscription(&self, _subscription: &UtxosChangedSubscription) -> Option<Self> {
        // TODO: apply the subscription
        Some(self.clone())
    }

    fn event_type(&self) -> EventType {
        self.into()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PruningPointUtxoSetOverrideNotification {}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    pub prefix: Prefix,
    pub added: Arc<UtxoSetByScriptPublicKey>,
    pub removed: Arc<UtxoSetByScriptPublicKey>,
}

impl UtxosChangedNotification {
    pub fn from_utxos_changed(utxos_changed: UtxoChanges, prefix: Prefix) -> Self {
        Self { added: Arc::new(utxos_changed.added), removed: Arc::new(utxos_changed.removed), prefix }
    }
}
