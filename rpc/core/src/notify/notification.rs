use super::{
    events::EventType,
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualSelectedParentChainChangedSubscription},
        Single,
    },
};
use std::fmt::{Debug, Display};

pub trait Notification: Clone + Debug + Display + Send + Sync + 'static {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription) -> Option<Self>;

    fn apply_virtual_selected_parent_chain_changed_subscription(
        &self,
        subscription: &VirtualSelectedParentChainChangedSubscription,
    ) -> Option<Self>;

    fn apply_utxos_changed_subscription(&self, subscription: &UtxosChangedSubscription) -> Option<Self>;

    fn apply_subscription(&self, subscription: &dyn Single) -> Option<Self> {
        match subscription.event_type() {
            EventType::VirtualSelectedParentChainChanged => self.apply_virtual_selected_parent_chain_changed_subscription(
                subscription.as_any().downcast_ref::<VirtualSelectedParentChainChangedSubscription>().unwrap(),
            ),
            EventType::UtxosChanged => {
                self.apply_utxos_changed_subscription(subscription.as_any().downcast_ref::<UtxosChangedSubscription>().unwrap())
            }
            _ => self.apply_overall_subscription(subscription.as_any().downcast_ref::<OverallSubscription>().unwrap()),
        }
    }

    fn event_type(&self) -> EventType;
}
