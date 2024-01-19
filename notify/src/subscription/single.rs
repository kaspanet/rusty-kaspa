use crate::{
    address::tracker::Indexes,
    events::EventType,
    listener::ListenerId,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::{
        context::SubscriptionContext, Command, DynSubscription, Mutation, MutationPolicies, Single, Subscription,
        UtxosChangedMutationPolicy,
    },
};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_core::trace;
use std::{
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
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
    fn mutated_and_mutations(
        &self,
        mutation: Mutation,
        _: MutationPolicies,
        _: &SubscriptionContext,
        _: ListenerId,
    ) -> Option<(DynSubscription, Vec<Mutation>)> {
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
    fn mutated_and_mutations(
        &self,
        mutation: Mutation,
        _: MutationPolicies,
        _: &SubscriptionContext,
        _: ListenerId,
    ) -> Option<(DynSubscription, Vec<Mutation>)> {
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

static UTXOS_CHANGED_SUBSCRIPTIONS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct UtxosChangedSubscription {
    active: bool,
    indexes: Indexes,
    listener_id: ListenerId,
}

impl UtxosChangedSubscription {
    pub fn new(active: bool, listener_id: ListenerId) -> Self {
        Self::with_capacity(active, listener_id, 0)
    }

    pub fn with_capacity(active: bool, listener_id: ListenerId, capacity: usize) -> Self {
        let cnt = UTXOS_CHANGED_SUBSCRIPTIONS.fetch_add(1, Ordering::SeqCst);
        let indexes = Indexes::new(Vec::with_capacity(capacity));
        let subscription = Self { active, indexes, listener_id };
        trace!("UtxosChangedSubscription: {} in total (new {})", cnt + 1, subscription);
        subscription
    }

    pub fn with_addresses(active: bool, addresses: &[Address], listener_id: ListenerId, context: &SubscriptionContext) -> Self {
        let mut subscription = Self::with_capacity(active, listener_id, addresses.len());
        let _ = subscription.register(addresses, context);
        subscription
    }

    pub fn contains(&self, spk: &ScriptPublicKey, context: &SubscriptionContext) -> bool {
        context.address_tracker.contains(&self.indexes, spk)
    }

    pub fn len(&self) -> usize {
        self.indexes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indexes.is_empty()
    }

    pub fn contains_address(&self, address: &Address, context: &SubscriptionContext) -> bool {
        context.address_tracker.contains_address(&self.indexes, address)
    }

    pub fn to_addresses(&self, prefix: Prefix, context: &SubscriptionContext) -> Vec<Address> {
        self.indexes.iter().filter_map(|index| context.address_tracker.get_index_address(*index, prefix)).collect_vec()
    }

    pub fn register(&mut self, addresses: &[Address], context: &SubscriptionContext) -> Vec<Address> {
        context.address_tracker.register(&mut self.indexes, addresses)
    }

    pub fn unregister(&mut self, addresses: &[Address], context: &SubscriptionContext) -> Vec<Address> {
        context.address_tracker.unregister(&mut self.indexes, addresses)
    }

    pub fn to_all(&self) -> bool {
        self.indexes.is_empty()
    }
}

impl Default for UtxosChangedSubscription {
    fn default() -> Self {
        let cnt = UTXOS_CHANGED_SUBSCRIPTIONS.fetch_add(1, Ordering::SeqCst);
        let subscription = Self::new(false, 0);
        trace!("UtxosChangedSubscription: {} in total (default {})", cnt + 1, subscription);
        subscription
    }
}

impl Clone for UtxosChangedSubscription {
    fn clone(&self) -> Self {
        let cnt = UTXOS_CHANGED_SUBSCRIPTIONS.fetch_add(1, Ordering::SeqCst);
        let subscription = Self { active: self.active, indexes: self.indexes.clone(), listener_id: self.listener_id };
        trace!("UtxosChangedSubscription: {} in total (clone {})", cnt + 1, subscription);
        subscription
    }
}

impl Display for UtxosChangedSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.active, self.indexes.len()) {
            (false, _) => write!(f, "none"),
            (true, 0) => write!(f, "all"),
            (true, 1) => write!(f, "1 address"),
            (true, n) => write!(f, "{} addresses", n),
        }
    }
}

impl Drop for UtxosChangedSubscription {
    fn drop(&mut self) {
        let cnt = UTXOS_CHANGED_SUBSCRIPTIONS.fetch_sub(1, Ordering::SeqCst);
        trace!("UtxosChangedSubscription: {} in total (drop {})", cnt - 1, self);
    }
}

impl PartialEq for UtxosChangedSubscription {
    fn eq(&self, other: &Self) -> bool {
        self.active == other.active && (!self.active || self.to_all() || (self.listener_id == other.listener_id))
    }
}
impl Eq for UtxosChangedSubscription {}

impl Hash for UtxosChangedSubscription {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.active.hash(state);
        if self.active && !self.to_all() {
            self.listener_id.hash(state);
        }
    }
}

impl Single for UtxosChangedSubscription {
    fn mutated_and_mutations(
        &self,
        mutation: Mutation,
        policies: MutationPolicies,
        context: &SubscriptionContext,
        listener_id: ListenerId,
    ) -> Option<(DynSubscription, Vec<Mutation>)> {
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
                    let mut mutated = Self::with_capacity(true, listener_id, scope.addresses.len());
                    let addresses = mutated.register(&scope.addresses, context);
                    let mutations = match policies.utxo_changed {
                        UtxosChangedMutationPolicy::AddressSet => {
                            Some(vec![Mutation::new(mutation.command, UtxosChangedScope::new(addresses).into())])
                        }
                        UtxosChangedMutationPolicy::AllOrNothing => {
                            Some(vec![Mutation::new(mutation.command, UtxosChangedScope::default().into())])
                        }
                    };
                    mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                }
            } else if !self.to_all() {
                // State Selected(S)
                if !mutation.active() {
                    if scope.addresses.is_empty() {
                        // Mutation None
                        let mutated = Self::new(false, listener_id);
                        context.address_tracker.unregister_indexes(&self.indexes);
                        let mutations = match policies.utxo_changed {
                            UtxosChangedMutationPolicy::AddressSet => Some(vec![Mutation::new(
                                Command::Stop,
                                UtxosChangedScope::new(self.to_addresses(Prefix::Mainnet, context)).into(), // FIXME
                            )]),
                            UtxosChangedMutationPolicy::AllOrNothing => {
                                Some(vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())])
                            }
                        };
                        mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                    } else {
                        // Mutation Remove(R)
                        if scope.addresses.iter().any(|address| self.contains_address(address, context)) {
                            let mut mutated = (*self).clone();
                            let removed = mutated.unregister(&scope.addresses, context);
                            mutated.active = !mutated.to_all();
                            let mutations = match (policies.utxo_changed, mutated.active) {
                                (UtxosChangedMutationPolicy::AddressSet, _) => {
                                    Some(vec![Mutation::new(Command::Stop, UtxosChangedScope::new(removed).into())])
                                }
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
                        if scope.addresses.iter().any(|address| !self.contains_address(address, context)) {
                            let mut mutated = (*self).clone();
                            let added = mutated.register(&scope.addresses, context);
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
                        let mutated = Self::new(true, listener_id);
                        let mutations = match policies.utxo_changed {
                            UtxosChangedMutationPolicy::AddressSet => Some(vec![
                                Mutation::new(
                                    Command::Stop,
                                    UtxosChangedScope::new(self.to_addresses(Prefix::Mainnet, context)).into(),
                                ), // FIXME
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
                        let mutated = Self::new(false, listener_id);
                        let mutations = Some(vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())]);
                        mutations.map(|x| (Arc::new(mutated) as DynSubscription, x))
                    } else {
                        // Mutation Remove(R)
                        None
                    }
                } else {
                    if !scope.addresses.is_empty() {
                        // Mutation Add(A)
                        let mut mutated = Self::with_capacity(true, listener_id, scope.addresses.len());
                        let added = mutated.register(&scope.addresses, context);
                        let mutations = match policies.utxo_changed {
                            UtxosChangedMutationPolicy::AddressSet => Some(vec![
                                Mutation::new(Command::Start, UtxosChangedScope::new(added).into()),
                                Mutation::new(Command::Stop, UtxosChangedScope::default().into()),
                            ]),
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
        //UtxosChangedScope::new(self.to_addresses(Prefix::Mainnet, context)).into()
        UtxosChangedScope::new(vec![]).into() // FIXME
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
            fn compare(&self, name: &str, subscriptions: &[DynSubscription]) {
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
            subscriptions: Vec<DynSubscription>,
            comparisons: Vec<Comparison>,
        }

        let context = SubscriptionContext::new();
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
                comparisons: vec![Comparison::new(0, 1, false), Comparison::new(0, 2, false), Comparison::new(1, 2, true)],
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
                    Arc::new(UtxosChangedSubscription::with_addresses(false, &[], 0, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, &addresses[0..2], 1, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, &addresses[0..3], 2, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, &sorted_addresses[0..3], 2, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, &[], 3, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, &[], 4, &context)),
                ],
                comparisons: vec![
                    Comparison::new(0, 0, true),
                    Comparison::new(0, 1, false),
                    Comparison::new(0, 2, false),
                    Comparison::new(0, 3, false),
                    Comparison::new(0, 4, false),
                    Comparison::new(0, 5, false),
                    Comparison::new(1, 1, true),
                    Comparison::new(1, 2, false),
                    Comparison::new(1, 3, false),
                    Comparison::new(1, 4, false),
                    Comparison::new(1, 5, false),
                    Comparison::new(2, 2, true),
                    Comparison::new(2, 3, true),
                    Comparison::new(2, 4, false),
                    Comparison::new(2, 5, false),
                    Comparison::new(3, 3, true),
                    Comparison::new(3, 4, false),
                    Comparison::new(3, 5, false),
                    Comparison::new(4, 4, true),
                    Comparison::new(4, 5, true),
                    Comparison::new(5, 5, true),
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
        pub const LISTENER_ID: ListenerId = 1;

        fn new(tests: Vec<MutationTest>) -> Self {
            Self { tests }
        }

        fn run(&self, context: &SubscriptionContext) {
            for test in self.tests.iter() {
                let mut new_state = test.state.clone();
                let result = new_state.mutate(test.mutation.clone(), Default::default(), context, Self::LISTENER_ID);
                assert_eq!(test.new_state.active(), new_state.active(), "Testing '{}': wrong new state activity", test.name);
                assert_eq!(*test.new_state, *new_state, "Testing '{}': wrong new state", test.name);
                assert_eq!(test.result, result, "Testing '{}': wrong result", test.name);
            }
        }
    }

    #[test]
    fn test_overall_mutation() {
        let context = SubscriptionContext::new();

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
        tests.run(&context)
    }

    #[test]
    fn test_virtual_chain_changed_mutation() {
        let context = SubscriptionContext::new();

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
        tests.run(&context)
    }

    #[test]
    fn test_utxos_changed_mutation() {
        let context = SubscriptionContext::new();
        let a_stock = get_3_addresses(true);

        let av = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let ah = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let s = |active: bool, indexes: &[usize]| {
            Arc::new(UtxosChangedSubscription::with_addresses(active, &ah(indexes), MutationTests::LISTENER_ID, &context))
                as DynSubscription
        };
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
        tests.run(&context)
    }
}
