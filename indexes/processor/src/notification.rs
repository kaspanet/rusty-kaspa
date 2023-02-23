use derive_more::Display;
use kaspa_notify::{full_featured, notification::Notification as NotificationTrait, subscription::Single};
use std::sync::Arc;
use utxoindex::model::{UtxoChanges, UtxoSetByScriptPublicKey};

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
    fn apply_overall_subscription(&self, subscription: &kaspa_notify::subscription::single::OverallSubscription) -> Option<Self> {
        match subscription.active() {
            true => Some(self.clone()),
            false => None,
        }
    }

    fn apply_virtual_selected_parent_chain_changed_subscription(
        &self,
        _subscription: &kaspa_notify::subscription::single::VirtualSelectedParentChainChangedSubscription,
    ) -> Option<Self> {
        Some(self.clone())
    }

    fn apply_utxos_changed_subscription(
        &self,
        _subscription: &kaspa_notify::subscription::single::UtxosChangedSubscription,
    ) -> Option<Self> {
        // TODO: apply the subscription
        Some(self.clone())
    }

    fn event_type(&self) -> kaspa_notify::events::EventType {
        self.into()
    }
}

#[derive(Debug, Clone, Default)]
pub struct PruningPointUtxoSetOverrideNotification {}

#[derive(Debug, Clone)]
pub struct UtxosChangedNotification {
    pub added: Arc<UtxoSetByScriptPublicKey>,
    pub removed: Arc<UtxoSetByScriptPublicKey>,
}

impl From<UtxoChanges> for UtxosChangedNotification {
    fn from(item: UtxoChanges) -> Self {
        Self { added: Arc::new(item.added), removed: Arc::new(item.removed) }
    }
}
