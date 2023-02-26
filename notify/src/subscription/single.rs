use super::{Mutation, Single, Subscription};
use crate::{
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualSelectedParentChainChangedScope},
    subscription::Command,
};
use addresses::Address;
use std::{
    collections::HashSet,
    fmt::Debug,
    hash::{Hash, Hasher},
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
    pub fn include_accepted_transaction_ids(&self) -> bool {
        self.include_accepted_transaction_ids
    }
}

impl Single for VirtualSelectedParentChainChangedSubscription {
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
                    Some(vec![Mutation::new(
                        Command::Stop,
                        Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::new(false)),
                    )])
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
                    Some(vec![Mutation::new(
                        Command::Stop,
                        Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::new(true)),
                    )])
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
    // TODO: handle address/script_public_key pairs
    //       this will be possible when txscript will have golang PayToAddrScript and ExtractScriptPubKeyAddress ported
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
                        if self.addresses.is_empty() {
                            self.active = false;
                        }
                        match removed.is_empty() {
                            false => Some(vec![Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed)))]),
                            true => None,
                        }
                    }
                } else {
                    if !scope.addresses.is_empty() {
                        // Mutation Add(A)
                        let added =
                            scope.addresses.iter().filter(|x| self.addresses.insert((*x).clone())).cloned().collect::<Vec<_>>();
                        match added.is_empty() {
                            false => Some(vec![Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::new(added)))]),
                            true => None,
                        }
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
    use crate::{address::test_helpers::get_3_addresses, scope::BlockAddedScope};
    use std::collections::hash_map::DefaultHasher;

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

        let addresses = get_3_addresses(false);
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

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_subscription_mutation() {
        struct Test {
            name: &'static str,
            state: SingleSubscription,
            mutation: Mutation,
            new_state: SingleSubscription,
            result: Option<Vec<Mutation>>,
        }

        // OverallSubscription

        let os_none = Box::new(OverallSubscription { event_type: EventType::BlockAdded, active: false });
        let os_all = Box::new(OverallSubscription { event_type: EventType::BlockAdded, active: true });
        let om_start_all = Mutation { command: Command::Start, scope: Scope::BlockAdded(BlockAddedScope {}) };
        let om_stop_all = Mutation { command: Command::Stop, scope: Scope::BlockAdded(BlockAddedScope {}) };

        // VirtualSelectedParentChainChangedSubscription

        let vs_none =
            Box::new(VirtualSelectedParentChainChangedSubscription { active: false, include_accepted_transaction_ids: false });
        let vs_reduced =
            Box::new(VirtualSelectedParentChainChangedSubscription { active: true, include_accepted_transaction_ids: false });
        let vs_all = Box::new(VirtualSelectedParentChainChangedSubscription { active: true, include_accepted_transaction_ids: true });
        let vm_start_all = Mutation {
            command: Command::Start,
            scope: Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope {
                include_accepted_transaction_ids: true,
            }),
        };
        let vm_stop_all = Mutation {
            command: Command::Stop,
            scope: Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope {
                include_accepted_transaction_ids: true,
            }),
        };
        let vm_start_reduced = Mutation {
            command: Command::Start,
            scope: Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope {
                include_accepted_transaction_ids: false,
            }),
        };
        let vm_stop_reduced = Mutation {
            command: Command::Stop,
            scope: Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope {
                include_accepted_transaction_ids: false,
            }),
        };

        // UtxosChangedSubscription

        let addresses = get_3_addresses(true);
        let a0 = HashSet::from_iter(vec![addresses[0].clone()].into_iter());
        let a1 = HashSet::from_iter(vec![addresses[1].clone()].into_iter());
        let a2 = HashSet::from_iter(vec![addresses[2].clone()].into_iter());
        let a01: HashSet<Address> = HashSet::from_iter(vec![addresses[0].clone(), addresses[1].clone()].into_iter());
        let a02: HashSet<Address> = HashSet::from_iter(vec![addresses[0].clone(), addresses[2].clone()].into_iter());
        let a012: HashSet<Address> = HashSet::from_iter(addresses.clone().into_iter());

        let us_none = Box::new(UtxosChangedSubscription { active: false, addresses: HashSet::default() });
        let us_selected_0 = Box::new(UtxosChangedSubscription { active: true, addresses: a0.clone() });
        let us_selected_1 = Box::new(UtxosChangedSubscription { active: true, addresses: a1.clone() });
        let us_selected_2 = Box::new(UtxosChangedSubscription { active: true, addresses: a2.clone() });
        let us_selected_01 = Box::new(UtxosChangedSubscription { active: true, addresses: a01.clone() });
        let us_selected_02 = Box::new(UtxosChangedSubscription { active: true, addresses: a02.clone() });
        let us_selected_012 = Box::new(UtxosChangedSubscription { active: true, addresses: a012.clone() });
        let us_all = Box::new(UtxosChangedSubscription { active: true, addresses: HashSet::default() });

        let um_start_all = Mutation { command: Command::Start, scope: Scope::UtxosChanged(UtxosChangedScope { addresses: vec![] }) };
        let um_stop_all = Mutation { command: Command::Stop, scope: Scope::UtxosChanged(UtxosChangedScope { addresses: vec![] }) };
        let um_start_0 = Mutation {
            command: Command::Start,
            scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a0.iter().cloned().collect() }),
        };
        let um_start_1 = Mutation {
            command: Command::Start,
            scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a1.iter().cloned().collect() }),
        };
        let um_start_01 = Mutation {
            command: Command::Start,
            scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a01.iter().cloned().collect() }),
        };
        let um_stop_0 = Mutation {
            command: Command::Stop,
            scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a0.iter().cloned().collect() }),
        };
        let um_stop_1 = Mutation {
            command: Command::Stop,
            scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a1.iter().cloned().collect() }),
        };
        let um_stop_01 = Mutation {
            command: Command::Stop,
            scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a01.iter().cloned().collect() }),
        };

        let tests: Vec<Test> = vec![
            //
            // OverallSubscription
            //
            Test {
                name: "OverallSubscription None to All",
                state: os_none.clone_box(),
                mutation: om_start_all.clone(),
                new_state: os_all.clone_box(),
                result: Some(vec![om_start_all.clone()]),
            },
            Test {
                name: "OverallSubscription None to None",
                state: os_none.clone_box(),
                mutation: om_stop_all.clone(),
                new_state: os_none.clone_box(),
                result: None,
            },
            Test {
                name: "OverallSubscription All to All",
                state: os_all.clone_box(),
                mutation: om_start_all.clone(),
                new_state: os_all.clone_box(),
                result: None,
            },
            Test {
                name: "OverallSubscription All to None",
                state: os_all.clone_box(),
                mutation: om_stop_all.clone(),
                new_state: os_none.clone_box(),
                result: Some(vec![om_stop_all.clone()]),
            },
            //
            // VirtualSelectedParentChainChangedSubscription
            //
            Test {
                name: "VirtualSelectedParentChainChangedSubscription None to All",
                state: vs_none.clone_box(),
                mutation: vm_start_all.clone(),
                new_state: vs_all.clone_box(),
                result: Some(vec![vm_start_all.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription None to Reduced",
                state: vs_none.clone_box(),
                mutation: vm_start_reduced.clone(),
                new_state: vs_reduced.clone_box(),
                result: Some(vec![vm_start_reduced.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription None to None (stop reduced)",
                state: vs_none.clone_box(),
                mutation: vm_stop_reduced.clone(),
                new_state: vs_none.clone_box(),
                result: None,
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription None to None (stop all)",
                state: vs_none.clone_box(),
                mutation: vm_stop_all.clone(),
                new_state: vs_none.clone_box(),
                result: None,
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription Reduced to All",
                state: vs_reduced.clone_box(),
                mutation: vm_start_all.clone(),
                new_state: vs_all.clone_box(),
                result: Some(vec![vm_stop_reduced.clone(), vm_start_all.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription Reduced to Reduced",
                state: vs_reduced.clone_box(),
                mutation: vm_start_reduced.clone(),
                new_state: vs_reduced.clone_box(),
                result: None,
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription Reduced to None (stop reduced)",
                state: vs_reduced.clone_box(),
                mutation: vm_stop_reduced.clone(),
                new_state: vs_none.clone_box(),
                result: Some(vec![vm_stop_reduced.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription Reduced to None (stop all)",
                state: vs_reduced.clone_box(),
                mutation: vm_stop_all.clone(),
                new_state: vs_none.clone_box(),
                result: Some(vec![vm_stop_reduced.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription All to All",
                state: vs_all.clone_box(),
                mutation: vm_start_all.clone(),
                new_state: vs_all.clone_box(),
                result: None,
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription All to Reduced",
                state: vs_all.clone_box(),
                mutation: vm_start_reduced.clone(),
                new_state: vs_reduced.clone_box(),
                result: Some(vec![vm_start_reduced.clone(), vm_stop_all.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription All to None (stop reduced)",
                state: vs_all.clone_box(),
                mutation: vm_stop_reduced.clone(),
                new_state: vs_none.clone_box(),
                result: Some(vec![vm_stop_all.clone()]),
            },
            Test {
                name: "VirtualSelectedParentChainChangedSubscription All to None (stop all)",
                state: vs_all.clone_box(),
                mutation: vm_stop_all.clone(),
                new_state: vs_none.clone_box(),
                result: Some(vec![vm_stop_all.clone()]),
            },
            //
            // UtxosChangedSubscription
            //
            Test {
                name: "UtxosChangedSubscription None to All (add all)",
                state: us_none.clone_box(),
                mutation: um_start_all.clone(),
                new_state: us_all.clone_box(),
                result: Some(vec![um_start_all.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription None to Selected 0 (add set)",
                state: us_none.clone_box(),
                mutation: um_start_0.clone(),
                new_state: us_selected_0.clone_box(),
                result: Some(vec![um_start_0.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription None to None (stop set)",
                state: us_none.clone_box(),
                mutation: um_stop_0.clone(),
                new_state: us_none.clone_box(),
                result: None,
            },
            Test {
                name: "UtxosChangedSubscription None to None (stop all)",
                state: us_none.clone_box(),
                mutation: um_stop_all.clone(),
                new_state: us_none.clone_box(),
                result: None,
            },
            Test {
                name: "UtxosChangedSubscription Selected 01 to All (add all)",
                state: us_selected_01.clone_box(),
                mutation: um_start_all.clone(),
                new_state: us_all.clone_box(),
                result: Some(vec![um_stop_01.clone(), um_start_all.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription Selected 01 to 01 (add set with total intersection)",
                state: us_selected_01.clone_box(),
                mutation: um_start_1.clone(),
                new_state: us_selected_01.clone_box(),
                result: None,
            },
            Test {
                name: "UtxosChangedSubscription Selected 0 to 01 (add set with partial intersection)",
                state: us_selected_0.clone_box(),
                mutation: um_start_01.clone(),
                new_state: us_selected_01.clone_box(),
                result: Some(vec![um_start_1.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription Selected 2 to 012 (add set with no intersection)",
                state: us_selected_2.clone_box(),
                mutation: um_start_01.clone(),
                new_state: us_selected_012.clone_box(),
                result: Some(vec![um_start_01.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription Selected 01 to None (remove superset)",
                state: us_selected_1.clone_box(),
                mutation: um_stop_01.clone(),
                new_state: us_none.clone_box(),
                result: Some(vec![um_stop_1.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription Selected 01 to None (remove set with total intersection)",
                state: us_selected_01.clone_box(),
                mutation: um_stop_01.clone(),
                new_state: us_none.clone_box(),
                result: Some(vec![um_stop_01.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription Selected 02 to 2 (remove set with partial intersection)",
                state: us_selected_02.clone_box(),
                mutation: um_stop_01.clone(),
                new_state: us_selected_2.clone_box(),
                result: Some(vec![um_stop_0.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription Selected 02 to 02 (remove set with no intersection)",
                state: us_selected_02.clone_box(),
                mutation: um_stop_1.clone(),
                new_state: us_selected_02.clone_box(),
                result: None,
            },
            Test {
                name: "UtxosChangedSubscription All to All (add all)",
                state: us_all.clone_box(),
                mutation: um_start_all.clone(),
                new_state: us_all.clone_box(),
                result: None,
            },
            Test {
                name: "UtxosChangedSubscription All to Selected 01 (add set)",
                state: us_all.clone_box(),
                mutation: um_start_01.clone(),
                new_state: us_selected_01.clone_box(),
                result: Some(vec![um_start_01.clone(), um_stop_all.clone()]),
            },
            Test {
                name: "UtxosChangedSubscription All to All (remove set)",
                state: us_all.clone_box(),
                mutation: um_stop_01.clone(),
                new_state: us_all.clone_box(),
                result: None,
            },
            Test {
                name: "UtxosChangedSubscription All to None (remove all)",
                state: us_all.clone_box(),
                mutation: um_stop_all.clone(),
                new_state: us_none.clone_box(),
                result: Some(vec![um_stop_all.clone()]),
            },
        ];

        for test in tests.iter() {
            let mut new_state = test.state.clone_box();
            let result = new_state.mutate(test.mutation.clone());
            assert_eq!(test.new_state.active(), new_state.active(), "Testing '{}': wrong new state activity", test.name);
            assert_eq!(*test.new_state, *new_state, "Testing '{}': wrong new state", test.name);
            assert_eq!(test.result, result, "Testing '{}': wrong result", test.name);
        }
    }
}
