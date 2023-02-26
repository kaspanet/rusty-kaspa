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

    fn apply_virtual_chain_changed_subscription(&self, subscription: &VirtualSelectedParentChainChangedSubscription) -> Option<Self>;

    fn apply_utxos_changed_subscription(&self, subscription: &UtxosChangedSubscription) -> Option<Self>;

    fn apply_subscription(&self, subscription: &dyn Single) -> Option<Self> {
        match subscription.event_type() {
            EventType::VirtualSelectedParentChainChanged => self.apply_virtual_chain_changed_subscription(
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
