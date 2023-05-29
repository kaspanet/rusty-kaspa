use crate::indexed_utxos::{UtxoChanges, UtxoSetByScriptPublicKey};
use derive_more::Display;
use kaspa_notify::{
    events::EventType,
    full_featured,
    notification::Notification as NotificationTrait,
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
        Subscription,
    },
};
use std::{collections::HashMap, sync::Arc};

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

    fn apply_utxos_changed_subscription(&self, subscription: &UtxosChangedSubscription) -> Option<Self> {
        match subscription.active() {
            true => {
                let Self::UtxosChanged(notification) = self else { return None };
                notification.apply_utxos_changed_subscription(subscription).map(Self::UtxosChanged)
            }
            false => None,
        }
    }

    fn event_type(&self) -> EventType {
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

impl UtxosChangedNotification {
    pub fn from_utxos_changed(utxos_changed: UtxoChanges) -> Self {
        Self { added: Arc::new(utxos_changed.added), removed: Arc::new(utxos_changed.removed) }
    }

    pub(crate) fn apply_utxos_changed_subscription(&self, subscription: &UtxosChangedSubscription) -> Option<Self> {
        if subscription.to_all() {
            Some(self.clone())
        } else {
            let added = Self::filter_utxo_set(&self.added, subscription);
            let removed = Self::filter_utxo_set(&self.removed, subscription);
            if added.is_empty() && removed.is_empty() {
                None
            } else {
                Some(Self { added: Arc::new(added), removed: Arc::new(removed) })
            }
        }
    }

    fn filter_utxo_set(utxo_set: &UtxoSetByScriptPublicKey, subscription: &UtxosChangedSubscription) -> UtxoSetByScriptPublicKey {
        // As an optimization, we iterate over the smaller set (O(n)) among the two below
        // and check existence over the larger set (O(1))
        let mut result = HashMap::default();
        if utxo_set.len() < subscription.addresses().len() {
            utxo_set.iter().for_each(|(script_public_key, collection)| {
                if subscription.addresses().contains_key(script_public_key) {
                    result.insert(script_public_key.clone(), collection.clone());
                }
            });
        } else {
            subscription.addresses().iter().filter(|(script_public_key, _)| utxo_set.contains_key(script_public_key)).for_each(
                |(script_public_key, _)| {
                    if let Some(collection) = utxo_set.get(script_public_key) {
                        result.insert(script_public_key.clone(), collection.clone());
                    }
                },
            );
        }
        result
    }
}
