use super::{DynSubscription, Mutation, MutationPolicies, Single, Subscription, UtxosChangedMutationPolicy};
use crate::{
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::Command,
};
use itertools::Itertools;
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_txscript::pay_to_address_script;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};
use uuid::Uuid;

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
    fn mutated_and_mutations(&self, mutation: Mutation, _: MutationPolicies) -> Option<(DynSubscription, Vec<Mutation>)> {
        assert_eq!(self.event_type(), mutation.event_type());
        if self.active != mutation.active() {
            let mutated = Self::new(self.event_type, mutation.active());
            Some((Arc::new(mutated), vec![mutation]))
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

    #[inline(always)]
    fn active(&self) -> bool {
        self.active
    }

    fn scope(&self) -> Scope {
        self.event_type.into()
    }
}

/// Subscription to VirtualChainChanged notifications
#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct VirtualChainChangedSubscription {
    active: bool,
    include_accepted_transaction_ids: bool,
}

impl VirtualChainChangedSubscription {
    pub fn new(active: bool, include_accepted_transaction_ids: bool) -> Self {
        Self { active, include_accepted_transaction_ids }
    }
    pub fn include_accepted_transaction_ids(&self) -> bool {
        self.include_accepted_transaction_ids
    }
}

impl Single for VirtualChainChangedSubscription {
    fn mutated_and_mutations(&self, mutation: Mutation, _: MutationPolicies) -> Option<(DynSubscription, Vec<Mutation>)> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::VirtualChainChanged(ref scope) = mutation.scope {
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
                    let mutated = Self::new(true, scope.include_accepted_transaction_ids);
                    Some((Arc::new(mutated), vec![mutation]))
                }
            } else if !self.include_accepted_transaction_ids {
                // State Reduced
                if !mutation.active() {
                    // Mutation None
                    let mutated = Self::new(false, false);
                    Some((Arc::new(mutated), vec![Mutation::new(Command::Stop, VirtualChainChangedScope::new(false).into())]))
                } else if !scope.include_accepted_transaction_ids {
                    // Mutation Reduced
                    None
                } else {
                    // Mutation All
                    let mutated = Self::new(true, true);
                    Some((
                        Arc::new(mutated),
                        vec![Mutation::new(Command::Stop, VirtualChainChangedScope::new(false).into()), mutation],
                    ))
                }
            } else {
                // State All
                if !mutation.active() {
                    // Mutation None
                    let mutated = Self::new(false, false);
                    Some((Arc::new(mutated), vec![Mutation::new(Command::Stop, VirtualChainChangedScope::new(true).into())]))
                } else if !scope.include_accepted_transaction_ids {
                    // Mutation Reduced
                    let mutated = Self::new(true, false);
                    Some((Arc::new(mutated), vec![mutation, Mutation::new(Command::Stop, VirtualChainChangedScope::new(true).into())]))
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

impl Subscription for VirtualChainChangedSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        EventType::VirtualChainChanged
    }

    #[inline(always)]
    fn active(&self) -> bool {
        self.active
    }

    fn scope(&self) -> Scope {
        VirtualChainChangedScope::new(self.include_accepted_transaction_ids).into()
    }
}

#[derive(Clone, Debug, Default)]
pub struct UtxosChangedSubscription {
    active: bool,
    addresses: HashMap<ScriptPublicKey, Address>,
    id: Uuid,
}

impl UtxosChangedSubscription {
    pub fn new(active: bool, addresses: Vec<Address>) -> Self {
        let mut subscription = Self { active, addresses: HashMap::default(), id: Uuid::new_v4() };
        subscription.set_addresses(addresses);
        subscription
    }

    fn set_addresses(&mut self, addresses: Vec<Address>) -> &mut Self {
        self.addresses = addresses.into_iter().map(|x| (pay_to_address_script(&x), x)).collect();
        self
    }

    pub fn insert_address(&mut self, address: &Address) -> bool {
        self.addresses.insert(pay_to_address_script(address), address.clone()).is_none()
    }

    pub fn contains_address(&self, address: &Address) -> bool {
        self.addresses.contains_key(&pay_to_address_script(address))
    }

    pub fn remove_address(&mut self, address: &Address) -> bool {
        self.addresses.remove(&pay_to_address_script(address)).is_some()
    }

    pub fn addresses(&self) -> &HashMap<ScriptPublicKey, Address> {
        &self.addresses
    }

    pub fn to_all(&self) -> bool {
        self.addresses.is_empty()
    }
}

impl PartialEq for UtxosChangedSubscription {
    fn eq(&self, other: &Self) -> bool {
        if self.active == other.active && self.addresses.len() == other.addresses.len() {
            // HashMaps are considered equal if they contain the same keys
            let result = self.addresses.keys().all(|x| other.addresses.contains_key(x));
            return result;
        }
        false
    }
}
impl Eq for UtxosChangedSubscription {}

impl Hash for UtxosChangedSubscription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.active.hash(state);

        // For non-blanket active subscriptions, every subscription is considered as unique since
        // it is extremely unlikely that two subscriptions may share an equal address set.
        if self.active && !self.addresses.is_empty() {
            self.id.hash(state);
        }
    }
}

impl Single for UtxosChangedSubscription {
    fn mutated_and_mutations(&self, mutation: Mutation, policies: MutationPolicies) -> Option<(DynSubscription, Vec<Mutation>)> {
        assert_eq!(self.event_type(), mutation.event_type());
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
                    let mutated = Self::new(true, scope.addresses.clone());
                    let mutations = match policies.utxo_changed {
                        UtxosChangedMutationPolicy::AddressSet => Some(vec![mutation]),
                        UtxosChangedMutationPolicy::AllOrNothing => {
                            Some(vec![Mutation::new(mutation.command, UtxosChangedScope::default().into())])
                        }
                    };
                    mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                }
            } else if !self.addresses.is_empty() {
                // State Selected(S)
                if !mutation.active() {
                    if scope.addresses.is_empty() {
                        // Mutation None
                        let mutated = Self::new(false, vec![]);
                        let mutations = match policies.utxo_changed {
                            UtxosChangedMutationPolicy::AddressSet => Some(vec![Mutation::new(
                                Command::Stop,
                                UtxosChangedScope::new(self.addresses.values().cloned().collect_vec()).into(),
                            )]),
                            UtxosChangedMutationPolicy::AllOrNothing => {
                                Some(vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())])
                            }
                        };
                        mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                    } else {
                        // Mutation Remove(R)
                        let removed = scope.addresses.iter().filter(|x| self.contains_address(x)).cloned().collect::<HashSet<_>>();
                        if !removed.is_empty() {
                            let addresses = self
                                .addresses
                                .values()
                                .filter_map(|x| if removed.contains(x) { None } else { Some(x.clone()) })
                                .collect_vec();
                            let mutated = Self::new(!addresses.is_empty(), addresses);
                            let mutations = match (policies.utxo_changed, mutated.active) {
                                (UtxosChangedMutationPolicy::AddressSet, _) => Some(vec![Mutation::new(
                                    Command::Stop,
                                    UtxosChangedScope::new(removed.into_iter().collect_vec()).into(),
                                )]),
                                (UtxosChangedMutationPolicy::AllOrNothing, false) => {
                                    Some(vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())])
                                }
                                (UtxosChangedMutationPolicy::AllOrNothing, true) => None,
                            };
                            mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                        } else {
                            None
                        }
                    }
                } else {
                    if !scope.addresses.is_empty() {
                        // Mutation Add(A)
                        let added = scope.addresses.iter().filter(|x| !self.contains_address(x)).cloned().collect_vec();
                        if !added.is_empty() {
                            let addresses = added.iter().cloned().chain(self.addresses.values().cloned()).collect_vec();
                            let mutated = Self::new(true, addresses);
                            let mutations = match policies.utxo_changed {
                                UtxosChangedMutationPolicy::AddressSet => {
                                    Some(vec![Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::new(added)))])
                                }
                                UtxosChangedMutationPolicy::AllOrNothing => None,
                            };
                            mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                        } else {
                            None
                        }
                    } else {
                        // Mutation All
                        let mutated = Self::new(true, vec![]);
                        let mutations = match policies.utxo_changed {
                            UtxosChangedMutationPolicy::AddressSet => Some(vec![
                                Mutation::new(
                                    Command::Stop,
                                    UtxosChangedScope::new(self.addresses.values().cloned().collect()).into(),
                                ),
                                Mutation::new(Command::Start, UtxosChangedScope::default().into()),
                            ]),
                            UtxosChangedMutationPolicy::AllOrNothing => None,
                        };
                        mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                    }
                }
            } else {
                // State All
                if !mutation.active() {
                    if scope.addresses.is_empty() {
                        // Mutation None
                        let mutated = Self::new(false, vec![]);
                        let mutations = Some(vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())]);
                        mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                    } else {
                        // Mutation Remove(R)
                        None
                    }
                } else {
                    if !scope.addresses.is_empty() {
                        // Mutation Add(A)
                        let mutated = Self::new(true, scope.addresses.clone());
                        let mutations = match policies.utxo_changed {
                            UtxosChangedMutationPolicy::AddressSet => {
                                Some(vec![mutation, Mutation::new(Command::Stop, UtxosChangedScope::default().into())])
                            }
                            UtxosChangedMutationPolicy::AllOrNothing => None,
                        };
                        mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
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

    fn active(&self) -> bool {
        self.active
    }

    fn scope(&self) -> Scope {
        UtxosChangedScope::new(self.addresses.values().cloned().collect()).into()
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
            hash_should_match: bool,
        }
        impl Comparison {
            fn new(left: usize, right: usize, should_match: bool, hash_should_match: bool) -> Self {
                Self { left, right, should_match, hash_should_match }
            }
            fn compare(&self, name: &str, subscriptions: &[DynSubscription]) {
                let equal = if self.should_match { "be equal" } else { "not be equal" };
                let equal_hash = if self.hash_should_match { "be equal" } else { "not be equal" };
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
                    self.hash_should_match,
                    "{name}: subscription hashes should {equal_hash}, comparing {:?} => {} with {:?} => {}",
                    &subscriptions[self.left],
                    get_hash(&subscriptions[self.left]),
                    &subscriptions[self.right],
                    get_hash(&subscriptions[self.right]),
                );
                // Compare Arc dyn Single
                let left_arc = subscriptions[self.left].clone();
                let right_arc = subscriptions[self.right].clone();
                assert_eq!(
                    *left_arc == *right_arc,
                    self.should_match,
                    "{name}: subscriptions should {equal}, comparing {left_arc:?} with {right_arc:?}",
                );
                // Compare Arc dyn Single hash
                assert_eq!(
                    get_hash(&left_arc) == get_hash(&right_arc),
                    self.hash_should_match,
                    "{name}: subscription hashes should {equal_hash}, comparing {:?} => {} with {:?} => {}",
                    left_arc,
                    get_hash(&left_arc),
                    right_arc,
                    get_hash(&right_arc),
                );
            }
        }

        struct Test {
            name: &'static str,
            subscriptions: Vec<DynSubscription>,
            comparisons: Vec<Comparison>,
        }

        let addresses = get_3_addresses(false);
        let mut sorted_addresses = addresses.clone();
        sorted_addresses.sort();

        let tests: Vec<Test> = vec![
            Test {
                name: "test basic overall subscription",
                subscriptions: vec![
                    Arc::new(OverallSubscription::new(EventType::BlockAdded, false)),
                    Arc::new(OverallSubscription::new(EventType::BlockAdded, true)),
                    Arc::new(OverallSubscription::new(EventType::BlockAdded, true)),
                ],
                comparisons: vec![
                    Comparison::new(0, 1, false, false),
                    Comparison::new(0, 2, false, false),
                    Comparison::new(1, 2, true, true),
                ],
            },
            Test {
                name: "test virtual selected parent chain changed subscription",
                subscriptions: vec![
                    Arc::new(VirtualChainChangedSubscription::new(false, false)),
                    Arc::new(VirtualChainChangedSubscription::new(true, false)),
                    Arc::new(VirtualChainChangedSubscription::new(true, true)),
                    Arc::new(VirtualChainChangedSubscription::new(true, true)),
                ],
                comparisons: vec![
                    Comparison::new(0, 1, false, false),
                    Comparison::new(0, 2, false, false),
                    Comparison::new(0, 3, false, false),
                    Comparison::new(1, 2, false, false),
                    Comparison::new(1, 3, false, false),
                    Comparison::new(2, 3, true, true),
                ],
            },
            Test {
                name: "test utxos changed subscription",
                subscriptions: vec![
                    Arc::new(UtxosChangedSubscription::new(false, vec![])),
                    Arc::new(UtxosChangedSubscription::new(true, addresses[0..2].to_vec())),
                    Arc::new(UtxosChangedSubscription::new(true, addresses[0..3].to_vec())),
                    Arc::new(UtxosChangedSubscription::new(true, sorted_addresses[0..3].to_vec())),
                    Arc::new(UtxosChangedSubscription::new(true, vec![])),
                    Arc::new(UtxosChangedSubscription::new(true, vec![])),
                ],
                comparisons: vec![
                    Comparison::new(0, 1, false, false),
                    Comparison::new(0, 2, false, false),
                    Comparison::new(0, 3, false, false),
                    Comparison::new(1, 2, false, false),
                    Comparison::new(1, 3, false, false),
                    Comparison::new(3, 3, true, true),
                    Comparison::new(0, 4, false, false),
                    Comparison::new(4, 5, true, true),
                    Comparison::new(2, 3, true, false), // same address sets but diverging ids
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

    struct MutationTest {
        name: &'static str,
        state: DynSubscription,
        mutation: Mutation,
        new_state: DynSubscription,
        result: Option<Vec<Mutation>>,
    }

    struct MutationTests {
        tests: Vec<MutationTest>,
    }

    impl MutationTests {
        fn new(tests: Vec<MutationTest>) -> Self {
            Self { tests }
        }

        fn run(&self) {
            for test in self.tests.iter() {
                let mut new_state = test.state.clone();
                let result = new_state.mutate(test.mutation.clone(), Default::default());
                assert_eq!(test.new_state.active(), new_state.active(), "Testing '{}': wrong new state activity", test.name);
                assert_eq!(*test.new_state, *new_state, "Testing '{}': wrong new state", test.name);
                assert_eq!(test.result, result, "Testing '{}': wrong result", test.name);
            }
        }
    }

    #[test]
    fn test_overall_mutation() {
        fn s(active: bool) -> DynSubscription {
            Arc::new(OverallSubscription { event_type: EventType::BlockAdded, active })
        }
        fn m(command: Command) -> Mutation {
            Mutation { command, scope: Scope::BlockAdded(BlockAddedScope {}) }
        }

        // Subscriptions
        let none = || s(false);
        let all = || s(true);

        // Mutations
        let start_all = || m(Command::Start);
        let stop_all = || m(Command::Stop);

        // Tests
        let tests = MutationTests::new(vec![
            MutationTest {
                name: "OverallSubscription None to All",
                state: none(),
                mutation: start_all(),
                new_state: all(),
                result: Some(vec![start_all()]),
            },
            MutationTest {
                name: "OverallSubscription None to None",
                state: none(),
                mutation: stop_all(),
                new_state: none(),
                result: None,
            },
            MutationTest {
                name: "OverallSubscription All to All",
                state: all(),
                mutation: start_all(),
                new_state: all(),
                result: None,
            },
            MutationTest {
                name: "OverallSubscription All to None",
                state: all(),
                mutation: stop_all(),
                new_state: none(),
                result: Some(vec![stop_all()]),
            },
        ]);
        tests.run()
    }

    #[test]
    fn test_virtual_chain_changed_mutation() {
        fn s(active: bool, include_accepted_transaction_ids: bool) -> DynSubscription {
            Arc::new(VirtualChainChangedSubscription { active, include_accepted_transaction_ids })
        }
        fn m(command: Command, include_accepted_transaction_ids: bool) -> Mutation {
            Mutation { command, scope: Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }) }
        }

        // Subscriptions
        let none = || s(false, false);
        let reduced = || s(true, false);
        let all = || s(true, true);

        // Mutations
        let start_all = || m(Command::Start, true);
        let stop_all = || m(Command::Stop, true);
        let start_reduced = || m(Command::Start, false);
        let stop_reduced = || m(Command::Stop, false);

        // Tests
        let tests = MutationTests::new(vec![
            MutationTest {
                name: "VirtualChainChangedSubscription None to All",
                state: none(),
                mutation: start_all(),
                new_state: all(),
                result: Some(vec![start_all()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription None to Reduced",
                state: none(),
                mutation: start_reduced(),
                new_state: reduced(),
                result: Some(vec![start_reduced()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription None to None (stop reduced)",
                state: none(),
                mutation: stop_reduced(),
                new_state: none(),
                result: None,
            },
            MutationTest {
                name: "VirtualChainChangedSubscription None to None (stop all)",
                state: none(),
                mutation: stop_all(),
                new_state: none(),
                result: None,
            },
            MutationTest {
                name: "VirtualChainChangedSubscription Reduced to All",
                state: reduced(),
                mutation: start_all(),
                new_state: all(),
                result: Some(vec![stop_reduced(), start_all()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription Reduced to Reduced",
                state: reduced(),
                mutation: start_reduced(),
                new_state: reduced(),
                result: None,
            },
            MutationTest {
                name: "VirtualChainChangedSubscription Reduced to None (stop reduced)",
                state: reduced(),
                mutation: stop_reduced(),
                new_state: none(),
                result: Some(vec![stop_reduced()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription Reduced to None (stop all)",
                state: reduced(),
                mutation: stop_all(),
                new_state: none(),
                result: Some(vec![stop_reduced()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription All to All",
                state: all(),
                mutation: start_all(),
                new_state: all(),
                result: None,
            },
            MutationTest {
                name: "VirtualChainChangedSubscription All to Reduced",
                state: all(),
                mutation: start_reduced(),
                new_state: reduced(),
                result: Some(vec![start_reduced(), stop_all()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription All to None (stop reduced)",
                state: all(),
                mutation: stop_reduced(),
                new_state: none(),
                result: Some(vec![stop_all()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription All to None (stop all)",
                state: all(),
                mutation: stop_all(),
                new_state: none(),
                result: Some(vec![stop_all()]),
            },
        ]);
        tests.run()
    }

    #[test]
    fn test_utxos_changed_mutation() {
        let a_stock = get_3_addresses(true);

        let av = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let ah = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let s = |active: bool, indexes: &[usize]| Arc::new(UtxosChangedSubscription::new(active, ah(indexes))) as DynSubscription;
        let m = |command: Command, indexes: &[usize]| -> Mutation {
            Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope::new(av(indexes))) }
        };

        // Subscriptions
        let none = || s(false, &[]);
        let selected_0 = || s(true, &[0]);
        let selected_1 = || s(true, &[1]);
        let selected_2 = || s(true, &[2]);
        let selected_01 = || s(true, &[0, 1]);
        let selected_02 = || s(true, &[0, 2]);
        let selected_012 = || s(true, &[0, 1, 2]);
        let all = || s(true, &[]);

        // Mutations
        let start_all = || m(Command::Start, &[]);
        let stop_all = || m(Command::Stop, &[]);
        let start_0 = || m(Command::Start, &[0]);
        let start_1 = || m(Command::Start, &[1]);
        let start_01 = || m(Command::Start, &[0, 1]);
        let stop_0 = || m(Command::Stop, &[0]);
        let stop_1 = || m(Command::Stop, &[1]);
        let stop_01 = || m(Command::Stop, &[0, 1]);

        // Tests
        let tests = MutationTests::new(vec![
            MutationTest {
                name: "UtxosChangedSubscription None to All (add all)",
                state: none(),
                mutation: start_all(),
                new_state: all(),
                result: Some(vec![start_all()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription None to Selected 0 (add set)",
                state: none(),
                mutation: start_0(),
                new_state: selected_0(),
                result: Some(vec![start_0()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription None to None (stop set)",
                state: none(),
                mutation: stop_0(),
                new_state: none(),
                result: None,
            },
            MutationTest {
                name: "UtxosChangedSubscription None to None (stop all)",
                state: none(),
                mutation: stop_all(),
                new_state: none(),
                result: None,
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to All (add all)",
                state: selected_01(),
                mutation: start_all(),
                new_state: all(),
                result: Some(vec![stop_01(), start_all()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to 01 (add set with total intersection)",
                state: selected_01(),
                mutation: start_1(),
                new_state: selected_01(),
                result: None,
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 0 to 01 (add set with partial intersection)",
                state: selected_0(),
                mutation: start_01(),
                new_state: selected_01(),
                result: Some(vec![start_1()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 2 to 012 (add set with no intersection)",
                state: selected_2(),
                mutation: start_01(),
                new_state: selected_012(),
                result: Some(vec![start_01()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to None (remove superset)",
                state: selected_1(),
                mutation: stop_01(),
                new_state: none(),
                result: Some(vec![stop_1()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to None (remove set with total intersection)",
                state: selected_01(),
                mutation: stop_01(),
                new_state: none(),
                result: Some(vec![stop_01()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 02 to 2 (remove set with partial intersection)",
                state: selected_02(),
                mutation: stop_01(),
                new_state: selected_2(),
                result: Some(vec![stop_0()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 02 to 02 (remove set with no intersection)",
                state: selected_02(),
                mutation: stop_1(),
                new_state: selected_02(),
                result: None,
            },
            MutationTest {
                name: "UtxosChangedSubscription All to All (add all)",
                state: all(),
                mutation: start_all(),
                new_state: all(),
                result: None,
            },
            MutationTest {
                name: "UtxosChangedSubscription All to Selected 01 (add set)",
                state: all(),
                mutation: start_01(),
                new_state: selected_01(),
                result: Some(vec![start_01(), stop_all()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription All to All (remove set)",
                state: all(),
                mutation: stop_01(),
                new_state: all(),
                result: None,
            },
            MutationTest {
                name: "UtxosChangedSubscription All to None (remove all)",
                state: all(),
                mutation: stop_all(),
                new_state: none(),
                result: Some(vec![stop_all()]),
            },
        ]);
        tests.run()
    }
}
