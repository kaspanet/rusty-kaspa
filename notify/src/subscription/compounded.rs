use crate::{
    address::{
        error::Result,
        tracker::{Counters, Tracker},
    },
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::{context::SubscriptionContext, Command, Compounded, Mutation, Subscription},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverallSubscription {
    event_type: EventType,
    active: usize,
}

impl OverallSubscription {
    pub fn new(event_type: EventType) -> Self {
        Self { event_type, active: 0 }
    }
}

impl Compounded for OverallSubscription {
    fn compound(&mut self, mutation: Mutation, _context: &SubscriptionContext) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        match mutation.command {
            Command::Start => {
                self.active += 1;
                if self.active == 1 {
                    return Some(mutation);
                }
            }
            Command::Stop => {
                assert!(self.active > 0);
                self.active -= 1;
                if self.active == 0 {
                    return Some(mutation);
                }
            }
        }
        None
    }
}

impl Subscription for OverallSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        self.event_type
    }

    fn active(&self) -> bool {
        self.active > 0
    }

    fn scope(&self, _context: &SubscriptionContext) -> Scope {
        self.event_type.into()
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct VirtualChainChangedSubscription {
    include_accepted_transaction_ids: [usize; 2],
}

impl VirtualChainChangedSubscription {
    #[inline(always)]
    fn all(&self) -> usize {
        self.include_accepted_transaction_ids[true as usize]
    }

    #[inline(always)]
    fn all_mut(&mut self) -> &mut usize {
        &mut self.include_accepted_transaction_ids[true as usize]
    }

    #[inline(always)]
    fn reduced(&self) -> usize {
        self.include_accepted_transaction_ids[false as usize]
    }

    #[inline(always)]
    fn reduced_mut(&mut self) -> &mut usize {
        &mut self.include_accepted_transaction_ids[false as usize]
    }
}

impl Compounded for VirtualChainChangedSubscription {
    fn compound(&mut self, mutation: Mutation, _context: &SubscriptionContext) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::VirtualChainChanged(ref scope) = mutation.scope {
            let all = scope.include_accepted_transaction_ids;
            match mutation.command {
                Command::Start => {
                    if all {
                        // Add All
                        *self.all_mut() += 1;
                        if self.all() == 1 {
                            return Some(mutation);
                        }
                    } else {
                        // Add Reduced
                        *self.reduced_mut() += 1;
                        if self.reduced() == 1 && self.all() == 0 {
                            return Some(mutation);
                        }
                    }
                }
                Command::Stop => {
                    if !all {
                        // Remove Reduced
                        assert!(self.reduced() > 0);
                        *self.reduced_mut() -= 1;
                        if self.reduced() == 0 && self.all() == 0 {
                            return Some(mutation);
                        }
                    } else {
                        // Remove All
                        assert!(self.all() > 0);
                        *self.all_mut() -= 1;
                        if self.all() == 0 {
                            if self.reduced() > 0 {
                                return Some(Mutation::new(
                                    Command::Start,
                                    Scope::VirtualChainChanged(VirtualChainChangedScope::new(false)),
                                ));
                            } else {
                                return Some(mutation);
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

impl Subscription for VirtualChainChangedSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        EventType::VirtualChainChanged
    }

    fn active(&self) -> bool {
        self.include_accepted_transaction_ids.iter().sum::<usize>() > 0
    }

    fn scope(&self, _context: &SubscriptionContext) -> Scope {
        Scope::VirtualChainChanged(VirtualChainChangedScope::new(self.all() > 0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UtxosChangedSubscription {
    all: usize,
    indexes: Counters,
}

impl UtxosChangedSubscription {
    pub fn new(tracker: Tracker) -> Self {
        Self { all: 0, indexes: Counters::new(tracker) }
    }

    pub fn with_capacity(tracker: Tracker, capacity: usize) -> Self {
        Self { all: 0, indexes: Counters::with_capacity(tracker, capacity) }
    }

    pub fn register(&mut self, scope: UtxosChangedScope) -> Result<UtxosChangedScope> {
        match scope {
            UtxosChangedScope::Addresses(addresses) => Ok(UtxosChangedScope::new(self.indexes.register(addresses)?)),
            UtxosChangedScope::Indexes(indexes) => Ok(UtxosChangedScope::new_indexes(self.indexes.register_indexes(indexes))),
        }
    }

    pub fn unregister(&mut self, scope: UtxosChangedScope) -> Result<UtxosChangedScope> {
        match scope {
            UtxosChangedScope::Addresses(addresses) => Ok(UtxosChangedScope::new(self.indexes.unregister(addresses)?)),
            UtxosChangedScope::Indexes(indexes) => Ok(UtxosChangedScope::new_indexes(self.indexes.unregister_indexes(indexes))),
        }
    }

    pub fn get_indexes(&self) -> UtxosChangedScope {
        UtxosChangedScope::new_indexes(self.indexes.get_indexes())
    }
}

impl Compounded for UtxosChangedSubscription {
    fn compound(&mut self, mutation: Mutation, _: &SubscriptionContext) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::UtxosChanged(scope) = mutation.scope {
            match mutation.command {
                Command::Start => {
                    if scope.is_empty() {
                        // Add All
                        self.all += 1;
                        if self.all == 1 {
                            return Some(Mutation::new(Command::Start, UtxosChangedScope::default().into()));
                        }
                    } else {
                        // Add(A)
                        let added = self.register(scope).expect("compounded always registers");
                        if !added.is_empty() && self.all == 0 {
                            return Some(Mutation::new(Command::Start, added.into()));
                        }
                    }
                }
                Command::Stop => {
                    if !scope.is_empty() {
                        // Remove(R)
                        let removed = self.unregister(scope).expect("compounded always unregisters");
                        if !removed.is_empty() && self.all == 0 {
                            return Some(Mutation::new(Command::Stop, removed.into()));
                        }
                    } else {
                        // Remove All
                        assert!(self.all > 0);
                        self.all -= 1;
                        if self.all == 0 {
                            let indexes = self.get_indexes();
                            if !indexes.is_empty() {
                                return Some(Mutation::new(Command::Start, indexes.into()));
                            } else {
                                return Some(Mutation::new(Command::Stop, UtxosChangedScope::default().into()));
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

impl Subscription for UtxosChangedSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        EventType::UtxosChanged
    }

    fn active(&self) -> bool {
        self.all > 0 || !self.indexes.is_empty()
    }

    fn scope(&self, _: &SubscriptionContext) -> Scope {
        let scope = if self.all > 0 { Default::default() } else { self.get_indexes() };
        scope.into()
    }
}

#[cfg(test)]
mod tests {
    use kaspa_core::trace;

    use super::super::*;
    use super::*;
    use crate::{
        address::test_helpers::{get_3_addresses, ADDRESS_PREFIX, NETWORK_TYPE},
        scope::BlockAddedScope,
    };
    use std::panic::AssertUnwindSafe;

    struct Step {
        name: &'static str,
        mutation: Mutation,
        result: Option<Mutation>,
    }

    struct Test {
        name: &'static str,
        context: SubscriptionContext,
        initial_state: CompoundedSubscription,
        steps: Vec<Step>,
        final_state: CompoundedSubscription,
    }

    impl Test {
        fn run(&self) -> CompoundedSubscription {
            let mut state = self.initial_state.clone_box();
            for (idx, step) in self.steps.iter().enumerate() {
                trace!("{}: {}", idx, step.name);
                let result = state.compound(step.mutation.clone(), &self.context);
                assert_eq!(step.result, result, "{} - {}: wrong compound result", self.name, step.name);
                trace!("{}: state = {:?}", idx, state);
            }
            assert_eq!(*self.final_state, *state, "{}: wrong final state", self.name);
            state
        }
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_overall_compounding() {
        let none = || Box::new(OverallSubscription::new(EventType::BlockAdded));
        let add = || Mutation::new(Command::Start, Scope::BlockAdded(BlockAddedScope {}));
        let remove = || Mutation::new(Command::Stop, Scope::BlockAdded(BlockAddedScope {}));
        let test = Test {
            name: "OverallSubscription 0 to 2 to 0",
            context: SubscriptionContext::new(Some(NETWORK_TYPE)),
            initial_state: none(),
            steps: vec![
                Step { name: "add 1", mutation: add(), result: Some(add()) },
                Step { name: "add 2", mutation: add(), result: None },
                Step { name: "remove 2", mutation: remove(), result: None },
                Step { name: "remove 1", mutation: remove(), result: Some(remove()) },
            ],
            final_state: none(),
        };
        let mut state = test.run();

        // Removing once more must panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove(), &test.context)));
        assert!(result.is_err(), "{}: trying to remove when counter is zero must panic", test.name);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_virtual_chain_changed_compounding() {
        fn m(command: Command, include_accepted_transaction_ids: bool) -> Mutation {
            Mutation { command, scope: Scope::VirtualChainChanged(VirtualChainChangedScope { include_accepted_transaction_ids }) }
        }
        let none = Box::<VirtualChainChangedSubscription>::default;
        let add_all = || m(Command::Start, true);
        let add_reduced = || m(Command::Start, false);
        let remove_reduced = || m(Command::Stop, false);
        let remove_all = || m(Command::Stop, true);
        let test = Test {
            name: "VirtualChainChanged",
            context: SubscriptionContext::new(Some(NETWORK_TYPE)),
            initial_state: none(),
            steps: vec![
                Step { name: "add all 1", mutation: add_all(), result: Some(add_all()) },
                Step { name: "add all 2", mutation: add_all(), result: None },
                Step { name: "remove all 2", mutation: remove_all(), result: None },
                Step { name: "remove all 1", mutation: remove_all(), result: Some(remove_all()) },
                Step { name: "add reduced 1", mutation: add_reduced(), result: Some(add_reduced()) },
                Step { name: "add reduced 2", mutation: add_reduced(), result: None },
                Step { name: "remove reduced 2", mutation: remove_reduced(), result: None },
                Step { name: "remove reduced 1", mutation: remove_reduced(), result: Some(remove_reduced()) },
                // Interleaved all and reduced
                Step { name: "add all 1", mutation: add_all(), result: Some(add_all()) },
                Step { name: "add reduced 1, masked by all", mutation: add_reduced(), result: None },
                Step { name: "remove all 1, revealing reduced", mutation: remove_all(), result: Some(add_reduced()) },
                Step { name: "add all 1, masking reduced", mutation: add_all(), result: Some(add_all()) },
                Step { name: "remove reduced 1, masked by all", mutation: remove_reduced(), result: None },
                Step { name: "remove all 1", mutation: remove_all(), result: Some(remove_all()) },
            ],
            final_state: none(),
        };
        let mut state = test.run();

        // Removing once more must panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_all(), &test.context)));
        assert!(result.is_err(), "{}: trying to remove all when counter is zero must panic", test.name);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_reduced(), &test.context)));
        assert!(result.is_err(), "{}: trying to remove reduced when counter is zero must panic", test.name);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_utxos_changed_compounding() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let context = SubscriptionContext::new(Some(NETWORK_TYPE));
        let a_stock = get_3_addresses(true);

        let a = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let m = |command: Command, indexes: &[usize]| -> Mutation {
            Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope::new(a(indexes))) }
        };
        let none = || Box::new(UtxosChangedSubscription::new(context.address_tracker.clone()));

        let add_all = || m(Command::Start, &[]);
        let remove_all = || m(Command::Stop, &[]);
        let add_0 = || m(Command::Start, &[0]);
        let add_1 = || m(Command::Start, &[1]);
        let add_01 = || m(Command::Start, &[0, 1]);
        let remove_0 = || m(Command::Stop, &[0]);
        let remove_1 = || m(Command::Stop, &[1]);

        let final_tracker = Tracker::new(Some(ADDRESS_PREFIX), None);
        let mut final_indexes = Counters::new(final_tracker.clone());
        final_indexes.register(a(&[0, 1])).unwrap();
        final_indexes.unregister(a(&[0, 1])).unwrap();

        let test = Test {
            name: "UtxosChanged",
            initial_state: none(),
            context,
            steps: vec![
                Step { name: "add all 1", mutation: add_all(), result: Some(add_all()) },
                Step { name: "add all 2", mutation: add_all(), result: None },
                Step { name: "remove all 2", mutation: remove_all(), result: None },
                Step { name: "remove all 1", mutation: remove_all(), result: Some(remove_all()) },
                Step { name: "add a0 1", mutation: add_0(), result: Some(add_0()) },
                Step { name: "add a0 2", mutation: add_0(), result: None },
                Step { name: "add a1 1", mutation: add_1(), result: Some(add_1()) },
                Step { name: "remove a0 2", mutation: remove_0(), result: None },
                Step { name: "remove a1 1", mutation: remove_1(), result: Some(remove_1()) },
                Step { name: "remove a0 1", mutation: remove_0(), result: Some(remove_0()) },
                // Interleaved all and address set
                Step { name: "add all 1", mutation: add_all(), result: Some(add_all()) },
                Step { name: "add a0a1, masked by all", mutation: add_01(), result: None },
                Step { name: "remove all 1, revealing a0a1", mutation: remove_all(), result: Some(add_01()) },
                Step { name: "add all 1, masking a0a1", mutation: add_all(), result: Some(add_all()) },
                Step { name: "remove a1, masked by all", mutation: remove_1(), result: None },
                Step { name: "remove all 1, revealing a0", mutation: remove_all(), result: Some(add_0()) },
                Step { name: "remove a0", mutation: remove_0(), result: Some(remove_0()) },
            ],
            final_state: Box::new(UtxosChangedSubscription { all: 0, indexes: final_indexes }),
        };
        let mut state = test.run();

        // Removing once more must panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_all(), &test.context)));
        assert!(result.is_err(), "{}: trying to remove all when counter is zero must panic", test.name);
        // let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_0(), &test.context)));
        // assert!(result.is_err(), "{}: trying to remove an address when its counter is zero must panic", test.name);
    }
}
