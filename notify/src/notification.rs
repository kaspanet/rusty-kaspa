use crate::subscription::context::SubscriptionContext;

use super::{
    events::EventType,
    subscription::{
        single::{OverallSubscription, UtxosChangedSubscription, VirtualChainChangedSubscription},
        Single,
    },
};
use std::fmt::{Debug, Display};

/// A notification, usable throughout the full notification system via types implementing this trait
pub trait Notification: Clone + Debug + Display + Send + Sync + 'static {
    fn apply_overall_subscription(&self, subscription: &OverallSubscription, context: &SubscriptionContext) -> Option<Self>;

    fn apply_virtual_chain_changed_subscription(
        &self,
        subscription: &VirtualChainChangedSubscription,
        context: &SubscriptionContext,
    ) -> Option<Self>;

    fn apply_utxos_changed_subscription(&self, subscription: &UtxosChangedSubscription, context: &SubscriptionContext)
        -> Option<Self>;

    fn apply_subscription(&self, subscription: &dyn Single, context: &SubscriptionContext) -> Option<Self> {
        match subscription.event_type() {
            EventType::VirtualChainChanged => self.apply_virtual_chain_changed_subscription(
                subscription.as_any().downcast_ref::<VirtualChainChangedSubscription>().unwrap(),
                context,
            ),
            EventType::UtxosChanged => self
                .apply_utxos_changed_subscription(subscription.as_any().downcast_ref::<UtxosChangedSubscription>().unwrap(), context),
            _ => self.apply_overall_subscription(subscription.as_any().downcast_ref::<OverallSubscription>().unwrap(), context),
        }
    }

    fn event_type(&self) -> EventType;
}

#[macro_export]
macro_rules! full_featured {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
    $($(#[$variant_meta:meta])* $variant_name:ident($field_name:path),)*
    }) => {
        paste::paste! {
        $(#[$meta])*
        $vis enum $name {
            $($(#[$variant_meta])* $variant_name($field_name)),*
        }

        impl std::convert::From<&$name> for kaspa_notify::events::EventType {
            fn from(value: &$name) -> Self {
                match value {
                    $($name::$variant_name(_) => kaspa_notify::events::EventType::$variant_name),*
                }
            }
        }

        impl std::convert::From<&$name> for kaspa_notify::scope::Scope {
            fn from(value: &$name) -> Self {
                match value {
                    $($name::$variant_name(_) => kaspa_notify::scope::Scope::$variant_name(kaspa_notify::scope::[<$variant_name Scope>]::default())),*
                }
            }
        }

        impl AsRef<$name> for $name {
            fn as_ref(&self) -> &Self {
                self
            }
        }
    }
    }
}

pub use full_featured;

pub mod test_helpers {
    use crate::subscription::{context::SubscriptionContext, Subscription};

    use super::*;
    use derive_more::Display;
    use kaspa_addresses::Address;
    use kaspa_core::trace;
    use std::sync::Arc;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct BlockAddedNotification {
        pub data: u64,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct VirtualChainChangedNotification {
        pub data: u64,
        pub accepted_transaction_ids: Option<u64>,
    }

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    pub struct UtxosChangedNotification {
        pub data: u64,
        pub addresses: Arc<Vec<Address>>,
    }

    full_featured! {
    #[derive(Clone, Debug, Display, PartialEq, Eq)]
    pub enum TestNotification {
        #[display(fmt = "BlockAdded #{}", "_0.data")]
        BlockAdded(BlockAddedNotification),
        #[display(fmt = "VirtualChainChanged #{}", "_0.data")]
        VirtualChainChanged(VirtualChainChangedNotification),
        #[display(fmt = "UtxosChanged #{}", "_0.data")]
        UtxosChanged(UtxosChangedNotification),
    }
    }

    impl Notification for TestNotification {
        fn apply_overall_subscription(&self, subscription: &OverallSubscription, _: &SubscriptionContext) -> Option<Self> {
            trace!("apply_overall_subscription: {self:?}, {subscription:?}");
            match subscription.active() {
                true => Some(self.clone()),
                false => None,
            }
        }

        fn apply_virtual_chain_changed_subscription(
            &self,
            subscription: &VirtualChainChangedSubscription,
            _: &SubscriptionContext,
        ) -> Option<Self> {
            match subscription.active() {
                true => {
                    if let TestNotification::VirtualChainChanged(ref payload) = self {
                        if !subscription.include_accepted_transaction_ids() && payload.accepted_transaction_ids.is_some() {
                            return Some(TestNotification::VirtualChainChanged(VirtualChainChangedNotification {
                                data: payload.data,
                                accepted_transaction_ids: None,
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
                    if let TestNotification::UtxosChanged(ref payload) = self {
                        let subscription = subscription.data();
                        if !subscription.to_all() {
                            // trace!("apply_utxos_changed_subscription: Notification payload {:?}", payload);
                            // trace!("apply_utxos_changed_subscription: Subscription content {:?}", subscription);
                            // trace!("apply_utxos_changed_subscription: Subscription Context {}", context.address_tracker);
                            let addresses = payload
                                .addresses
                                .iter()
                                .filter(|x| subscription.contains_address(x, context))
                                .cloned()
                                .collect::<Vec<_>>();
                            if !addresses.is_empty() {
                                return Some(TestNotification::UtxosChanged(UtxosChangedNotification {
                                    data: payload.data,
                                    addresses: Arc::new(addresses),
                                }));
                            } else {
                                return None;
                            }
                        }
                    }
                    Some(self.clone())
                }
                false => None,
            }
        }

        fn event_type(&self) -> EventType {
            self.into()
        }
    }

    /// A trait to help tests match notification received and expected thanks to some predefined data
    pub trait Data {
        fn data(&self) -> u64;
        fn data_mut(&mut self) -> &mut u64;
    }
    impl Data for BlockAddedNotification {
        fn data(&self) -> u64 {
            self.data
        }

        fn data_mut(&mut self) -> &mut u64 {
            &mut self.data
        }
    }
    impl Data for VirtualChainChangedNotification {
        fn data(&self) -> u64 {
            self.data
        }

        fn data_mut(&mut self) -> &mut u64 {
            &mut self.data
        }
    }
    impl Data for UtxosChangedNotification {
        fn data(&self) -> u64 {
            self.data
        }

        fn data_mut(&mut self) -> &mut u64 {
            &mut self.data
        }
    }
    impl Data for TestNotification {
        fn data(&self) -> u64 {
            match self {
                TestNotification::BlockAdded(n) => n.data(),
                TestNotification::VirtualChainChanged(n) => n.data(),
                TestNotification::UtxosChanged(n) => n.data(),
            }
        }

        fn data_mut(&mut self) -> &mut u64 {
            match self {
                TestNotification::BlockAdded(n) => n.data_mut(),
                TestNotification::VirtualChainChanged(n) => n.data_mut(),
                TestNotification::UtxosChanged(n) => n.data_mut(),
            }
        }
    }
}
