use super::{Mutation, Single, Subscription};
use crate::{
    address::UtxoAddress,
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::Command,
};
use kaspa_addresses::Address;
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_txscript::pay_to_address_script;
use std::{
    collections::HashMap,
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
    fn mutate(&mut self, mutation: Mutation) -> Option<Vec<Mutation>> {
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
                    Some(vec![Mutation::new(Command::Stop, Scope::VirtualChainChanged(VirtualChainChangedScope::new(false)))])
                } else if !scope.include_accepted_transaction_ids {
                    // Mutation Reduced
                    None
                } else {
                    // Mutation All
                    self.include_accepted_transaction_ids = true;
                    Some(vec![
                        Mutation::new(Command::Stop, Scope::VirtualChainChanged(VirtualChainChangedScope::new(false))),
                        mutation,
                    ])
                }
            } else {
                // State All
                if !mutation.active() {
                    // Mutation None
                    self.active = false;
                    self.include_accepted_transaction_ids = false;
                    Some(vec![Mutation::new(Command::Stop, Scope::VirtualChainChanged(VirtualChainChangedScope::new(true)))])
                } else if !scope.include_accepted_transaction_ids {
                    // Mutation Reduced
                    self.include_accepted_transaction_ids = false;
                    Some(vec![mutation, Mutation::new(Command::Stop, Scope::VirtualChainChanged(VirtualChainChangedScope::new(true)))])
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
        Scope::VirtualChainChanged(VirtualChainChangedScope::new(self.include_accepted_transaction_ids))
    }
}

#[derive(Clone, Debug, Default)]
pub struct UtxosChangedSubscription {
    active: bool,
    addresses: HashMap<ScriptPublicKey, UtxoAddress>,
}

impl UtxosChangedSubscription {
    pub fn new(active: bool, addresses: Vec<Address>) -> Self {
        let mut subscription = Self { active, addresses: HashMap::default() };
        subscription.set_addresses(addresses);
        subscription
    }

    fn set_addresses(&mut self, addresses: Vec<Address>) -> &mut Self {
        self.addresses = addresses
            .into_iter()
            .map(|x| {
                let utxo_address: UtxoAddress = x.into();
                (utxo_address.to_script_public_key(), utxo_address)
            })
            .collect();
        self
    }

    pub fn insert_address(&mut self, address: &Address) -> bool {
        let utxo_address: UtxoAddress = address.clone().into();
        self.addresses.insert(utxo_address.to_script_public_key(), utxo_address).is_none()
    }

    pub fn contains_address(&self, address: &Address) -> bool {
        self.addresses.contains_key(&pay_to_address_script(address))
    }

    pub fn remove_address(&mut self, address: &Address) -> bool {
        self.addresses.remove(&pay_to_address_script(address)).is_some()
    }

    pub fn addresses(&self) -> &HashMap<ScriptPublicKey, UtxoAddress> {
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
            return self.addresses.keys().all(|x| other.addresses.contains_key(x));
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
        let mut items: Vec<&Address> = self.addresses.values().map(|x| &**x).collect::<Vec<_>>();
        items.sort();
        items.hash(state);
    }
}

impl Single for UtxosChangedSubscription {
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
                    self.set_addresses(scope.addresses.clone());
                    Some(vec![mutation])
                }
            } else if !self.addresses.is_empty() {
                // State Selected(S)
                if !mutation.active() {
                    if scope.addresses.is_empty() {
                        // Mutation None
                        self.active = false;
                        let removed = self.addresses.drain().map(|(_, x)| x.into()).collect();
                        Some(vec![Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed)))])
                    } else {
                        // Mutation Remove(R)
                        let removed: Vec<Address> = scope.addresses.iter().filter(|x| self.remove_address(x)).cloned().collect();
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
                        let added = scope.addresses.iter().filter(|x| self.insert_address(x)).cloned().collect::<Vec<_>>();
                        match added.is_empty() {
                            false => Some(vec![Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::new(added)))]),
                            true => None,
                        }
                    } else {
                        // Mutation All
                        let removed: Vec<Address> = self.addresses.drain().map(|(_, x)| x.into()).collect();
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
                            self.insert_address(x);
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
}

impl Subscription for UtxosChangedSubscription {
    fn event_type(&self) -> EventType {
        EventType::UtxosChanged
    }

    fn active(&self) -> bool {
        self.active
    }

    fn scope(&self) -> Scope {
        Scope::UtxosChanged(UtxosChangedScope::new(self.addresses.values().map(|x| &**x).cloned().collect()))
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
                    Box::new(VirtualChainChangedSubscription::new(false, false)),
                    Box::new(VirtualChainChangedSubscription::new(true, false)),
                    Box::new(VirtualChainChangedSubscription::new(true, true)),
                    Box::new(VirtualChainChangedSubscription::new(true, true)),
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
                    Box::new(UtxosChangedSubscription::new(false, vec![])),
                    Box::new(UtxosChangedSubscription::new(true, addresses[0..2].to_vec())),
                    Box::new(UtxosChangedSubscription::new(true, addresses[0..3].to_vec())),
                    Box::new(UtxosChangedSubscription::new(true, sorted_addresses[0..3].to_vec())),
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

    struct MutationTest {
        name: &'static str,
        state: SingleSubscription,
        mutation: Mutation,
        new_state: SingleSubscription,
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
                let mut new_state = test.state.clone_box();
                let result = new_state.mutate(test.mutation.clone());
                assert_eq!(test.new_state.active(), new_state.active(), "Testing '{}': wrong new state activity", test.name);
                assert_eq!(*test.new_state, *new_state, "Testing '{}': wrong new state", test.name);
                assert_eq!(test.result, result, "Testing '{}': wrong result", test.name);
            }
        }
    }

    #[test]
    fn test_overall_mutation() {
        fn s(active: bool) -> SingleSubscription {
            Box::new(OverallSubscription { event_type: EventType::BlockAdded, active })
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
        fn s(active: bool, include_accepted_transaction_ids: bool) -> SingleSubscription {
            Box::new(VirtualChainChangedSubscription { active, include_accepted_transaction_ids })
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
        let s = |active: bool, indexes: &[usize]| Box::new(UtxosChangedSubscription::new(active, ah(indexes))) as SingleSubscription;
        let m = |command: Command, indexes: &[usize]| -> Mutation {
            Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope { addresses: av(indexes) }) }
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
