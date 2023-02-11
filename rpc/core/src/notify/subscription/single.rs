use addresses::Address;

use super::{Mutation, Single, Subscription};
use crate::{
    api::ops::SubscribeCommand, notify::events::EventType, Notification, NotificationType,
    VirtualSelectedParentChainChangedNotification,
};
use std::{
    collections::HashSet,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Subscription with a all or none scope.
///
/// To be used by all notifications which [`NotificationType`] variant is fieldless.
#[derive(Eq, PartialEq, Hash, Debug)]
pub struct OverallSubscription {
    event_type: EventType,
    active: bool,
}

impl OverallSubscription {
    pub fn new(event_type: EventType, active: bool) -> Self {
        Self { event_type, active }
    }
}

impl Single for OverallSubscription {
    fn apply_to(&self, notification: Arc<Notification>) -> Arc<Notification> {
        assert!(self.active);
        assert_eq!(self.event_type, (&*notification).into());
        notification
    }

    #[inline(always)]
    fn active(&self) -> bool {
        self.active
    }

    fn mutate(&mut self, mutation: Mutation) -> Option<Vec<Mutation>> {
        assert_eq!(self.event_type(), mutation.event_type());
        if self.active != mutation.active() {
            self.active = mutation.active();
            Some(vec![mutation])
        } else {
            None
        }
    }
}

impl Subscription for OverallSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        self.event_type
    }
}

/// Subscription to VirtualSelectedParentChainChanged notifications
#[derive(Eq, PartialEq, Hash, Debug, Default)]
pub struct VirtualSelectedParentChainChangedSubscription {
    active: bool,
    include_accepted_transaction_ids: bool,
}

impl VirtualSelectedParentChainChangedSubscription {
    pub fn new(active: bool, include_accepted_transaction_ids: bool) -> Self {
        Self { active, include_accepted_transaction_ids }
    }
}

impl Single for VirtualSelectedParentChainChangedSubscription {
    fn apply_to(&self, notification: Arc<Notification>) -> Arc<Notification> {
        assert!(self.active);
        assert_eq!(self.event_type(), (&*notification).into());
        if let Notification::VirtualSelectedParentChainChanged(ref payload) = *notification {
            if !self.include_accepted_transaction_ids && !payload.accepted_transaction_ids.is_empty() {
                return Arc::new(Notification::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedNotification {
                    removed_chain_block_hashes: payload.removed_chain_block_hashes.clone(),
                    added_chain_block_hashes: payload.added_chain_block_hashes.clone(),
                    accepted_transaction_ids: vec![],
                }));
            }
        }
        notification
    }

    #[inline(always)]
    fn active(&self) -> bool {
        self.active
    }

    fn mutate(&mut self, mutation: Mutation) -> Option<Vec<Mutation>> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let NotificationType::VirtualSelectedParentChainChanged(ref include_accepted_transaction_ids) = mutation.scope {
            // Here we want the code to (almost) match a double entry table structure
            // by subscription state and by mutation
            #[allow(clippy::collapsible_else_if)]
            if !self.active {
                // State None
                if !mutation.active() {
                    // Mutation None
                    None
                } else {
                    // Here is an exception to the aforementioned goal
                    // Mutations Reduced and All
                    self.active = true;
                    self.include_accepted_transaction_ids = *include_accepted_transaction_ids;
                    Some(vec![mutation])
                }
            } else if !self.include_accepted_transaction_ids {
                // State Reduced
                if !mutation.active() {
                    // Mutation None
                    self.active = false;
                    self.include_accepted_transaction_ids = false;
                    Some(vec![mutation])
                } else if !include_accepted_transaction_ids {
                    // Mutation Reduced
                    None
                } else {
                    // Mutation All
                    self.include_accepted_transaction_ids = true;
                    Some(vec![
                        Mutation::new(SubscribeCommand::Stop, NotificationType::VirtualSelectedParentChainChanged(false)),
                        mutation,
                    ])
                }
            } else {
                // State All
                if !mutation.active() {
                    // Mutation None
                    self.active = false;
                    self.include_accepted_transaction_ids = false;
                    Some(vec![mutation])
                } else if !include_accepted_transaction_ids {
                    // Mutation Reduced
                    self.include_accepted_transaction_ids = false;
                    Some(vec![
                        mutation,
                        Mutation::new(SubscribeCommand::Stop, NotificationType::VirtualSelectedParentChainChanged(true)),
                    ])
                } else {
                    // Mutation All
                    None
                }
            }
        } else {
            None
        }
    }
}

impl Subscription for VirtualSelectedParentChainChangedSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        EventType::VirtualSelectedParentChainChanged
    }
}

#[derive(Debug, Default)]
pub struct UtxosChangedSubscription {
    active: bool,
    addresses: HashSet<Address>,
}

impl PartialEq for UtxosChangedSubscription {
    fn eq(&self, other: &Self) -> bool {
        if self.active == other.active && self.addresses.len() == other.addresses.len() {
            // HashSets are equal if they contain the same elements
            return self.addresses.iter().all(|x| other.addresses.contains(x));
        }
        false
    }
}
impl Eq for UtxosChangedSubscription {}

impl Hash for UtxosChangedSubscription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.active.hash(state);

        // Since item order in hash set is undefined, build a sorted vector
        // so that hashing is determinist.
        let mut items: Vec<&Address> = self.addresses.iter().collect::<Vec<_>>();
        items.sort();
        items.hash(state);
    }
}

impl Single for UtxosChangedSubscription {
    fn apply_to(&self, _notification: Arc<Notification>) -> Arc<Notification> {
        todo!()
    }

    fn active(&self) -> bool {
        self.active
    }

    fn mutate(&mut self, mutation: Mutation) -> Option<Vec<Mutation>> {
        if let NotificationType::UtxosChanged(ref addresses) = mutation.scope {
            // Here we want the code to (almost) match a double entry table structure
            // by subscription state and by mutation
            #[allow(clippy::collapsible_else_if)]
            if !self.active {
                // State None
                if !mutation.active() {
                    // Here is an exception to the aforementioned goal
                    // Mutations None and Remove(R)
                    None
                } else {
                    // Here is an exception to the aforementioned goal
                    // Mutations Add(A) && All
                    self.active = true;
                    self.addresses = addresses.iter().cloned().collect();
                    Some(vec![mutation])
                }
            } else if !self.addresses.is_empty() {
                // State Selected(S)
                if !mutation.active() {
                    if addresses.is_empty() {
                        // Mutation None
                        self.active = false;
                        let removed = self.addresses.drain().collect();
                        Some(vec![Mutation::new(SubscribeCommand::Stop, NotificationType::UtxosChanged(removed))])
                    } else {
                        // Mutation Remove(R)
                        let removed: Vec<Address> = addresses.iter().filter(|x| self.addresses.remove(x)).cloned().collect();
                        Some(vec![Mutation::new(SubscribeCommand::Stop, NotificationType::UtxosChanged(removed))])
                    }
                } else {
                    if !addresses.is_empty() {
                        // Mutation Add(A)
                        let added = addresses.iter().filter(|x| self.addresses.insert((*x).clone())).cloned().collect();
                        Some(vec![Mutation::new(SubscribeCommand::Start, NotificationType::UtxosChanged(added))])
                    } else {
                        // Mutation All
                        let removed: Vec<Address> = self.addresses.drain().collect();
                        Some(vec![
                            Mutation::new(SubscribeCommand::Stop, NotificationType::UtxosChanged(removed)),
                            Mutation::new(SubscribeCommand::Start, NotificationType::UtxosChanged(vec![])),
                        ])
                    }
                }
            } else {
                // State All
                if !mutation.active() {
                    if addresses.is_empty() {
                        // Mutation None
                        self.active = false;
                        Some(vec![Mutation::new(SubscribeCommand::Stop, NotificationType::UtxosChanged(vec![]))])
                    } else {
                        // Mutation Remove(R)
                        None
                    }
                } else {
                    if !addresses.is_empty() {
                        // Mutation Add(A)
                        addresses.iter().for_each(|x| {
                            self.addresses.insert((*x).clone());
                        });
                        Some(vec![mutation, Mutation::new(SubscribeCommand::Stop, NotificationType::UtxosChanged(vec![]))])
                    } else {
                        // Mutation All
                        None
                    }
                }
            }
        } else {
            None
        }
    }
}

impl Subscription for UtxosChangedSubscription {
    fn event_type(&self) -> EventType {
        EventType::UtxosChanged
    }
}

// #[derive(Debug)]
// pub struct SubsetSubscription<T>
// where
//     T: Eq + Ord + Hash + Debug + Send + Sync,
// {
//     event_type: EventType,
//     active: bool,
//     items: HashSet<T>,
// }

// impl<T> PartialEq for SubsetSubscription<T>
// where
//     T: Eq + Ord + Hash + Debug + Send + Sync,
// {
//     fn eq(&self, other: &Self) -> bool {
//         if self.active == other.active && self.items.len() == other.items.len() {
//             // HashSets are equal if they contain the same elements
//             return self.items.iter().all(|x| other.items.contains(x));
//         }
//         false
//     }
// }

// impl<T> Eq for SubsetSubscription<T> where T: Eq + Ord + Hash + Debug + Send + Sync {}

// impl<T> Hash for SubsetSubscription<T>
// where
//     T: Eq + Ord + Hash + Debug + Send + Sync,
// {
//     fn hash<H: Hasher>(&self, state: &mut H) {
//         self.active.hash(state);

//         // Since item order in hash set is undefined, build a sorted vector
//         // so that hashing is determinist.
//         let mut items: Vec<&T> = self.items.iter().collect::<Vec<_>>();
//         items.sort();
//         items.hash(state);
//     }
// }

// impl<T> SingleSubscription for SubsetSubscription<T>
// where
//     T: Eq + Ord + Hash + Debug + Send + Sync + 'static,
// {
//     fn apply_to(&self, _notification: Arc<Notification>) -> Arc<Notification> {
//         todo!()
//     }

//     fn active(&self) -> bool {
//         self.active
//     }

//     fn mutate(&mut self, _mutation: Mutation) -> Option<Vec<Mutation>> {
//         todo!()
//     }
// }

// impl<T> Subscription for SubsetSubscription<T>
// where
//     T: Eq + Ord + Hash + Debug + Send + Sync + 'static,
// {
//     fn event_type(&self) -> EventType {
//         self.event_type
//     }
// }

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use addresses::Prefix;
    use std::collections::hash_map::DefaultHasher;

    fn addresses() -> Vec<Address> {
        vec![
            Address { prefix: Prefix::Mainnet, payload: vec![2u8; 32], version: 0 },
            Address { prefix: Prefix::Mainnet, payload: vec![3u8; 32], version: 0 },
            Address { prefix: Prefix::Mainnet, payload: vec![1u8; 32], version: 0 },
        ]
    }

    #[test]
    fn test_subscription_hash() {
        let g1 = OverallSubscription::new(EventType::BlockAdded, false);
        let g2 = OverallSubscription::new(EventType::BlockAdded, true);
        let g3 = OverallSubscription::new(EventType::BlockAdded, true);

        assert_ne!(g1, g2);
        assert_ne!(g1, g3);
        assert_eq!(g2, g3);

        assert_ne!(get_hash(&g1), get_hash(&g2));
        assert_ne!(get_hash(&g1), get_hash(&g3));
        assert_eq!(get_hash(&g2), get_hash(&g3));

        let s1: DynSingleSubscription = Box::new(g1);
        let s2: DynSingleSubscription = Box::new(g2);
        let s3: DynSingleSubscription = Box::new(g3);

        assert_ne!(*s1, *s2);
        assert_ne!(*s1, *s3);
        assert_eq!(*s2, *s3);

        assert_ne!(get_hash(&s1), get_hash(&s2));
        assert_ne!(get_hash(&s1), get_hash(&s3));
        assert_eq!(get_hash(&s2), get_hash(&s3));

        let h1: UtxosChangedSubscription = UtxosChangedSubscription { active: false, addresses: HashSet::default() };
        let mut addresses = addresses();
        let h2: UtxosChangedSubscription =
            UtxosChangedSubscription { active: true, addresses: addresses[0..2].iter().cloned().collect() };
        let h3: UtxosChangedSubscription =
            UtxosChangedSubscription { active: true, addresses: addresses[0..3].iter().cloned().collect() };
        addresses.sort();
        let h4: UtxosChangedSubscription =
            UtxosChangedSubscription { active: true, addresses: addresses[0..3].iter().cloned().collect() };

        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
        assert_ne!(h2, h3);
        assert_ne!(h2, h4);
        assert_eq!(h3, h4);

        let s1: DynSingleSubscription = Box::new(h1);
        let s2: DynSingleSubscription = Box::new(h2);
        let s3: DynSingleSubscription = Box::new(h3);
        let s4: DynSingleSubscription = Box::new(h4);

        assert_ne!(*s1, *s2);
        assert_ne!(*s1, *s3);
        assert_ne!(*s1, *s4);
        assert_ne!(*s2, *s3);
        assert_ne!(*s2, *s4);
        assert_eq!(*s3, *s4);

        assert_ne!(get_hash(&s1), get_hash(&s2));
        assert_ne!(get_hash(&s1), get_hash(&s3));
        assert_ne!(get_hash(&s1), get_hash(&s4));
        assert_ne!(get_hash(&s2), get_hash(&s3));
        assert_ne!(get_hash(&s2), get_hash(&s4));
        assert_eq!(get_hash(&s3), get_hash(&s4));
    }

    fn get_hash<T: Hash>(item: &T) -> u64 {
        let mut hasher = DefaultHasher::default();
        item.hash(&mut hasher);
        hasher.finish()
    }
}
