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

#[macro_export]
macro_rules! full_featured {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
    $($(#[$variant_meta:meta])* $variant_name:ident($field_name:path),)*
    }) => {
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
                    $name::BlockAdded(_) => Scope::BlockAdded,
                    $name::VirtualSelectedParentChainChanged(_) => {
                        Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::default())
                    }
                    $name::FinalityConflict(_) => Scope::FinalityConflict,
                    $name::FinalityConflictResolved(_) => Scope::FinalityConflictResolved,
                    $name::UtxosChanged(_) => Scope::UtxosChanged(UtxosChangedScope::default()),
                    $name::VirtualSelectedParentBlueScoreChanged(_) => Scope::VirtualSelectedParentBlueScoreChanged,
                    $name::VirtualDaaScoreChanged(_) => Scope::VirtualDaaScoreChanged,
                    $name::PruningPointUtxoSetOverride(_) => Scope::PruningPointUtxoSetOverride,
                    $name::NewBlockTemplate(_) => Scope::NewBlockTemplate,
                }
            }
        }

        impl AsRef<$name> for $name {
            fn as_ref(&self) -> &Self {
                self
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $name::BlockAdded(ref notification) => {
                        write!(f, "BlockAdded notification: block hash {}", notification.block.header.hash)
                    }
                    $name::NewBlockTemplate(_) => {
                        write!(f, "NewBlockTemplate notification")
                    }
                    $name::VirtualSelectedParentChainChanged(ref notification) => {
                        write!(
                            f,
                            "VirtualSelectedParentChainChanged notification: {} removed blocks, {} added blocks, {} accepted transactions",
                            notification.removed_chain_block_hashes.len(),
                            notification.added_chain_block_hashes.len(),
                            notification.accepted_transaction_ids.len()
                        )
                    }
                    $name::FinalityConflict(ref notification) => {
                        write!(f, "FinalityConflict notification: violating block hash {}", notification.violating_block_hash)
                    }
                    $name::FinalityConflictResolved(ref notification) => {
                        write!(f, "FinalityConflictResolved notification: finality block hash {}", notification.finality_block_hash)
                    }
                    $name::UtxosChanged(ref _notification) => {
                        write!(f, "UtxosChanged notification")
                    }
                    $name::VirtualSelectedParentBlueScoreChanged(ref notification) => {
                        write!(
                            f,
                            "VirtualSelectedParentBlueScoreChanged notification: virtual selected parent blue score {}",
                            notification.virtual_selected_parent_blue_score
                        )
                    }
                    $name::VirtualDaaScoreChanged(ref notification) => {
                        write!(f, "VirtualDaaScoreChanged notification: virtual DAA score {}", notification.virtual_daa_score)
                    }
                    $name::PruningPointUtxoSetOverride(_) => {
                        write!(f, "PruningPointUtxoSetOverride notification")
                    }
                }
            }
        }
    }
}

pub use full_featured;
