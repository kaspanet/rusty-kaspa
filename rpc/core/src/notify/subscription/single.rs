use super::{super::scope::Scope, Mutation, Single, Subscription};
use crate::{
    notify::{
        events::EventType,
        scope::{UtxosChangedScope, VirtualSelectedParentChainChangedScope},
        subscription::Command,
    },
    Notification, VirtualSelectedParentChainChangedNotification,
};
use addresses::Address;
use std::{
    collections::HashSet,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};

/// Subscription with a all or none scope.
///
/// To be used by all notifications which [`Scope`] variant is fieldless.
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
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

    fn scope(&self) -> Scope {
        self.event_type.into()
    }
}

impl Subscription for OverallSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        self.event_type
    }
}

/// Subscription to VirtualSelectedParentChainChanged notifications
#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
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
        if let Scope::VirtualSelectedParentChainChanged(ref scope) = mutation.scope {
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
                    self.include_accepted_transaction_ids = scope.include_accepted_transaction_ids;
                    Some(vec![mutation])
                }
            } else if !self.include_accepted_transaction_ids {
                // State Reduced
                if !mutation.active() {
                    // Mutation None
                    self.active = false;
                    self.include_accepted_transaction_ids = false;
                    Some(vec![mutation])
                } else if !scope.include_accepted_transaction_ids {
                    // Mutation Reduced
                    None
                } else {
                    // Mutation All
                    self.include_accepted_transaction_ids = true;
                    Some(vec![
                        Mutation::new(
                            Command::Stop,
                            Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::new(false)),
                        ),
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
                } else if !scope.include_accepted_transaction_ids {
                    // Mutation Reduced
                    self.include_accepted_transaction_ids = false;
                    Some(vec![
                        mutation,
                        Mutation::new(
                            Command::Stop,
                            Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::new(true)),
                        ),
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

    fn scope(&self) -> Scope {
        Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::new(self.include_accepted_transaction_ids))
    }
}

impl Subscription for VirtualSelectedParentChainChangedSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        EventType::VirtualSelectedParentChainChanged
    }
}

#[derive(Clone, Debug, Default)]
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
        if let Scope::UtxosChanged(ref scope) = mutation.scope {
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
                    self.addresses = scope.addresses.iter().cloned().collect();
                    Some(vec![mutation])
                }
            } else if !self.addresses.is_empty() {
                // State Selected(S)
                if !mutation.active() {
                    if scope.addresses.is_empty() {
                        // Mutation None
                        self.active = false;
                        let removed = self.addresses.drain().collect();
                        Some(vec![Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed)))])
                    } else {
                        // Mutation Remove(R)
                        let removed: Vec<Address> = scope.addresses.iter().filter(|x| self.addresses.remove(x)).cloned().collect();
                        Some(vec![Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed)))])
                    }
                } else {
                    if !scope.addresses.is_empty() {
                        // Mutation Add(A)
                        let added = scope.addresses.iter().filter(|x| self.addresses.insert((*x).clone())).cloned().collect();
                        Some(vec![Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::new(added)))])
                    } else {
                        // Mutation All
                        let removed: Vec<Address> = self.addresses.drain().collect();
                        Some(vec![
                            Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed))),
                            Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::default())),
                        ])
                    }
                }
            } else {
                // State All
                if !mutation.active() {
                    if scope.addresses.is_empty() {
                        // Mutation None
                        self.active = false;
                        Some(vec![Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::default()))])
                    } else {
                        // Mutation Remove(R)
                        None
                    }
                } else {
                    if !scope.addresses.is_empty() {
                        // Mutation Add(A)
                        scope.addresses.iter().for_each(|x| {
                            self.addresses.insert((*x).clone());
                        });
                        Some(vec![mutation, Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::default()))])
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

    fn scope(&self) -> Scope {
        Scope::UtxosChanged(UtxosChangedScope::new(self.addresses.iter().cloned().collect()))
    }
}

impl Subscription for UtxosChangedSubscription {
    fn event_type(&self) -> EventType {
        EventType::UtxosChanged
    }
}

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
        struct Comparison {
            left: usize,
            right: usize,
            should_match: bool,
        }
        impl Comparison {
            fn new(left: usize, right: usize, should_match: bool) -> Self {
                Self { left, right, should_match }
            }
            fn compare(&self, name: &str, subscriptions: &[SingleSubscription]) {
                let equal = if self.should_match { "be equal" } else { "not be equal" };
                // Compare Box dyn Single
                #[allow(clippy::op_ref)]
                let cmp = &subscriptions[self.left] == &subscriptions[self.right];
                assert_eq!(
                    cmp, self.should_match,
                    "{name}: subscriptions should {equal}, comparing {:?} with {:?}",
                    &subscriptions[self.left], &subscriptions[self.right],
                );
                // Compare Box dyn Single hash
                assert_eq!(
                    get_hash(&subscriptions[self.left]) == get_hash(&subscriptions[self.right]),
                    self.should_match,
                    "{name}: subscription hashes should {equal}, comparing {:?} => {} with {:?} => {}",
                    &subscriptions[self.left],
                    get_hash(&subscriptions[self.left]),
                    &subscriptions[self.right],
                    get_hash(&subscriptions[self.right]),
                );
                // Compare Arc dyn Single
                let left_arc = subscriptions[self.left].clone_arc();
                let right_arc = subscriptions[self.right].clone_arc();
                assert_eq!(
                    *left_arc == *right_arc,
                    self.should_match,
                    "{name}: subscriptions should {equal}, comparing {left_arc:?} with {right_arc:?}",
                );
                // Compare Arc dyn Single hash
                assert_eq!(
                    get_hash(&left_arc) == get_hash(&right_arc),
                    self.should_match,
                    "{name}: subscription hashes should {equal}, comparing {:?} => {} with {:?} => {}",
                    left_arc,
                    get_hash(&left_arc),
                    right_arc,
                    get_hash(&right_arc),
                );
            }
        }

        struct Test {
            name: &'static str,
            subscriptions: Vec<SingleSubscription>,
            comparisons: Vec<Comparison>,
        }

        let addresses = addresses();
        let mut sorted_addresses = addresses.clone();
        sorted_addresses.sort();

        let tests: Vec<Test> = vec![
            Test {
                name: "test basic overall subscription",
                subscriptions: vec![
                    Box::new(OverallSubscription::new(EventType::BlockAdded, false)),
                    Box::new(OverallSubscription::new(EventType::BlockAdded, true)),
                    Box::new(OverallSubscription::new(EventType::BlockAdded, true)),
                ],
                comparisons: vec![Comparison::new(0, 1, false), Comparison::new(0, 2, false), Comparison::new(1, 2, true)],
            },
            Test {
                name: "test virtual selected parent chain changed subscription",
                subscriptions: vec![
                    Box::new(VirtualSelectedParentChainChangedSubscription::new(false, false)),
                    Box::new(VirtualSelectedParentChainChangedSubscription::new(true, false)),
                    Box::new(VirtualSelectedParentChainChangedSubscription::new(true, true)),
                    Box::new(VirtualSelectedParentChainChangedSubscription::new(true, true)),
                ],
                comparisons: vec![
                    Comparison::new(0, 1, false),
                    Comparison::new(0, 2, false),
                    Comparison::new(0, 3, false),
                    Comparison::new(1, 2, false),
                    Comparison::new(1, 3, false),
                    Comparison::new(2, 3, true),
                ],
            },
            Test {
                name: "test utxos changed subscription",
                subscriptions: vec![
                    Box::new(UtxosChangedSubscription { active: false, addresses: HashSet::default() }),
                    Box::new(UtxosChangedSubscription { active: true, addresses: addresses[0..2].iter().cloned().collect() }),
                    Box::new(UtxosChangedSubscription { active: true, addresses: addresses[0..3].iter().cloned().collect() }),
                    Box::new(UtxosChangedSubscription { active: true, addresses: sorted_addresses[0..3].iter().cloned().collect() }),
                ],
                comparisons: vec![
                    Comparison::new(0, 1, false),
                    Comparison::new(0, 2, false),
                    Comparison::new(0, 3, false),
                    Comparison::new(1, 2, false),
                    Comparison::new(1, 3, false),
                    Comparison::new(3, 3, true),
                ],
            },
        ];

        for test in tests.iter() {
            for comparison in test.comparisons.iter() {
                comparison.compare(test.name, &test.subscriptions);
            }
        }
    }

    fn get_hash<T: Hash>(item: &T) -> u64 {
        let mut hasher = DefaultHasher::default();
        item.hash(&mut hasher);
        hasher.finish()
    }
}
