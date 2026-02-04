use crate::{
    address::{error::Result, tracker::Counters},
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::{Command, Compounded, Mutation, Subscription, context::SubscriptionContext},
};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};

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
    accepted_tx_counts: usize,
    blue_scores_counts: usize,
    active: usize,
}

impl VirtualChainChangedSubscription {
    #[inline(always)]
    fn inc(&mut self, is_new_active: bool, accepted_tx: bool, blue: bool) {
        if is_new_active {
            self.active += 1;
        }
        if accepted_tx {
            self.accepted_tx_counts += 1;
        }
        if blue {
            self.blue_scores_counts += 1;
        }
    }

    #[inline(always)]
    fn dec(&mut self, is_new_deactive: bool, accepted_tx: bool, blue: bool) {
        if is_new_deactive {
            self.active -= 1;
        }
        if accepted_tx {
            self.accepted_tx_counts -= 1;
        }
        if blue {
            self.blue_scores_counts -= 1;
        }
    }

    /// Returns (active_count, accepted_tx_count, blue_scores_count)
    fn snapshot(&self) -> (usize, usize, usize) {
        (self.active, self.accepted_tx_counts, self.blue_scores_counts)
    }
}

impl Compounded for VirtualChainChangedSubscription {
    fn compound(&mut self, mutation: Mutation, _context: &SubscriptionContext) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::VirtualChainChanged(ref scope) = mutation.scope {
            match mutation.command {
                Command::Start => {
                    // Snapshot previous totals
                    let (prev_total_active, prev_total_accepted_txs, prev_total_blue_scores) = self.snapshot();

                    // Apply mutation to self
                    self.inc(scope.active, scope.include_accepted_transaction_ids, scope.include_accepting_blue_scores);

                    // New totals
                    let (new_total_active, new_total_accepted_txs, new_total_blue_scores) = self.snapshot();

                    // Assert that at least one of the counters incremented, and incremented counters are not incremented by not more then one.
                    assert!(
                        (new_total_active - prev_total_active == 0 || new_total_active - prev_total_active == 1)
                            && (new_total_accepted_txs - prev_total_accepted_txs == 0
                                || new_total_accepted_txs - prev_total_accepted_txs == 1)
                            && (new_total_blue_scores - prev_total_blue_scores == 0
                                || new_total_blue_scores - prev_total_blue_scores == 1),
                        "Invalid VirtualChainChangedSubscription state after Start mutation: prev_total_active={}, new_total_active={}, prev_total_accepted_txs={}, new_total_accepted_txs={}, prev_total_blue_scores={}, new_total_blue_scores={}",
                        prev_total_active,
                        new_total_active,
                        prev_total_accepted_txs,
                        new_total_accepted_txs,
                        prev_total_blue_scores,
                        new_total_blue_scores,
                    );

                    let signal_new_total_accepted_txs = new_total_accepted_txs == 1 && prev_total_accepted_txs == 0;
                    let signal_new_total_blue_scores = new_total_blue_scores == 1 && prev_total_blue_scores == 0;
                    let signal_new_total_active = new_total_active == 1 && prev_total_active == 0;

                    if signal_new_total_active || signal_new_total_accepted_txs || signal_new_total_blue_scores {
                        return Some(Mutation::new(
                            Command::Start,
                            Scope::VirtualChainChanged(VirtualChainChangedScope::new(
                                signal_new_total_active,
                                signal_new_total_accepted_txs,
                                signal_new_total_blue_scores,
                            )),
                        ));
                    } else {
                        return None;
                    }
                }
                Command::Stop => {
                    // Snapshot previous totals
                    let (prev_total_active, prev_total_accepted_txs, prev_total_blue_scores) = self.snapshot();

                    // Apply mutation to self (decrement)
                    self.dec(scope.active, scope.include_accepted_transaction_ids, scope.include_accepting_blue_scores);

                    // New totals after decrement
                    let (new_total_active, new_total_accepted_txs, new_total_blue_scores) = self.snapshot();

                    // Assert that at least one of the counters decremented, and decremented counters are not decremented by more then one.
                    assert!(
                        (prev_total_active - new_total_active == 0 || prev_total_active - new_total_active == 1)
                            && (prev_total_accepted_txs - new_total_accepted_txs == 0
                                || prev_total_accepted_txs - new_total_accepted_txs == 1)
                            && (prev_total_blue_scores - new_total_blue_scores == 0
                                || prev_total_blue_scores - new_total_blue_scores == 1),
                        "Invalid VirtualChainChangedSubscription state after Stop mutation: prev_total_active={}, new_total_active={}, prev_total_accepted_txs={}, new_total_accepted_txs={}, prev_total_blue_scores={}, new_total_blue_scores={}",
                        prev_total_active,
                        new_total_active,
                        prev_total_accepted_txs,
                        new_total_accepted_txs,
                        prev_total_blue_scores,
                        new_total_blue_scores,
                    );

                    let signal_depleted_total_accepted_txs = new_total_accepted_txs == 0 && prev_total_accepted_txs == 1;
                    let signal_depleted_total_blue_scores = new_total_blue_scores == 0 && prev_total_blue_scores == 1;
                    let signal_depleted_total_active = new_total_active == 0 && prev_total_active == 1;

                    if signal_depleted_total_active || signal_depleted_total_accepted_txs || signal_depleted_total_blue_scores {
                        return Some(Mutation::new(
                            Command::Stop,
                            Scope::VirtualChainChanged(VirtualChainChangedScope::new(
                                signal_depleted_total_active,
                                signal_depleted_total_accepted_txs,
                                signal_depleted_total_blue_scores,
                            )),
                        ));
                    } else {
                        return None;
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
        self.active > 0
    }

    fn scope(&self, _context: &SubscriptionContext) -> Scope {
        Scope::VirtualChainChanged(VirtualChainChangedScope::new(
            self.active > 0,
            self.accepted_tx_counts > 0,
            self.blue_scores_counts > 0,
        ))
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct UtxosChangedSubscription {
    all: usize,
    indexes: Counters,
}

impl UtxosChangedSubscription {
    pub fn new() -> Self {
        Self { all: 0, indexes: Counters::new() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { all: 0, indexes: Counters::with_capacity(capacity) }
    }

    pub fn to_addresses(&self, prefix: Prefix, context: &SubscriptionContext) -> Vec<Address> {
        self.indexes
            .iter()
            .filter_map(|(&index, &count)| {
                (count > 0).then_some(()).and_then(|_| context.address_tracker.get_address_at_index(index, prefix))
            })
            .collect_vec()
    }

    pub fn register(&mut self, addresses: Vec<Address>, context: &SubscriptionContext) -> Result<Vec<Address>> {
        context.address_tracker.register(&mut self.indexes, addresses)
    }

    pub fn unregister(&mut self, addresses: Vec<Address>, context: &SubscriptionContext) -> Vec<Address> {
        context.address_tracker.unregister(&mut self.indexes, addresses)
    }
}

impl Compounded for UtxosChangedSubscription {
    fn compound(&mut self, mutation: Mutation, context: &SubscriptionContext) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::UtxosChanged(scope) = mutation.scope {
            match mutation.command {
                Command::Start => {
                    if scope.addresses.is_empty() {
                        // Add All
                        self.all += 1;
                        if self.all == 1 {
                            return Some(Mutation::new(Command::Start, UtxosChangedScope::default().into()));
                        }
                    } else {
                        // Add(A)
                        let added = self.register(scope.addresses, context).expect("compounded always registers");
                        if !added.is_empty() && self.all == 0 {
                            return Some(Mutation::new(Command::Start, UtxosChangedScope::new(added).into()));
                        }
                    }
                }
                Command::Stop => {
                    if !scope.addresses.is_empty() {
                        // Remove(R)
                        let removed = self.unregister(scope.addresses, context);
                        if !removed.is_empty() && self.all == 0 {
                            return Some(Mutation::new(Command::Stop, UtxosChangedScope::new(removed).into()));
                        }
                    } else {
                        // Remove All
                        assert!(self.all > 0);
                        self.all -= 1;
                        if self.all == 0 {
                            let addresses = self.to_addresses(Prefix::Mainnet, context);
                            if !addresses.is_empty() {
                                return Some(Mutation::new(Command::Start, UtxosChangedScope::new(addresses).into()));
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

    fn scope(&self, context: &SubscriptionContext) -> Scope {
        let addresses = if self.all > 0 { vec![] } else { self.to_addresses(Prefix::Mainnet, context) };
        Scope::UtxosChanged(UtxosChangedScope::new(addresses))
    }
}

#[cfg(test)]
mod tests {
    use kaspa_core::trace;

    use super::super::*;
    use super::*;
    use crate::{
        address::{test_helpers::get_3_addresses, tracker::Counter},
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
            context: SubscriptionContext::new(),
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
        fn m(command: Command, active: bool, include_accepted_transaction_ids: bool, include_accepting_blue_scores: bool) -> Mutation {
            Mutation {
                command,
                scope: Scope::VirtualChainChanged(VirtualChainChangedScope {
                    active,
                    include_accepted_transaction_ids,
                    include_accepting_blue_scores,
                }),
            }
        }
        let none = Box::<VirtualChainChangedSubscription>::default;
        // default blue flag is false for legacy behavior in these unit tests
        let command_builder =
            |command: Command, active: bool, include_accepted_transaction_ids: bool, include_accepting_blue_scores: bool| {
                m(command, active, include_accepted_transaction_ids, include_accepting_blue_scores)
            };

        let test = Test {
            name: "VirtualChainChanged",
            context: SubscriptionContext::new(),
            initial_state: none(),
            steps: vec![
                Step {
                    name: "add_all",
                    mutation: command_builder(Command::Start, true, true, true),
                    result: Some(command_builder(Command::Start, true, true, true)),
                },
                Step {
                    name: "remove transactions 1",
                    mutation: command_builder(Command::Stop, false, true, false),
                    result: Some(command_builder(Command::Stop, false, true, false)),
                },
                Step {
                    name: "remove blue score 1",
                    mutation: command_builder(Command::Stop, false, false, true),
                    result: Some(command_builder(Command::Stop, false, false, true)),
                },
                Step {
                    name: "add transactions / add blue score 1",
                    mutation: command_builder(Command::Start, false, true, true),
                    result: Some(command_builder(Command::Start, false, true, true)),
                },
                Step { name: "add active 1", mutation: command_builder(Command::Start, true, false, false), result: None },
                Step {
                    name: "remove transactions / remove blue score 1",
                    mutation: command_builder(Command::Stop, false, true, true),
                    result: Some(command_builder(Command::Stop, false, true, true)),
                },
                Step { name: "remove active1", mutation: command_builder(Command::Stop, true, false, false), result: None },
                Step {
                    name: "remove active2",
                    mutation: command_builder(Command::Stop, true, false, false),
                    result: Some(command_builder(Command::Stop, true, false, false)),
                },
                Step {
                    name: "start active+txids",
                    mutation: command_builder(Command::Start, true, true, false),
                    result: Some(command_builder(Command::Start, true, true, false)),
                },
                // 10: Start blue only (new blue -> emit Start blue)
                Step {
                    name: "start blue only",
                    mutation: command_builder(Command::Start, false, false, true),
                    result: Some(command_builder(Command::Start, false, false, true)),
                },
                Step { name: "start dup all", mutation: command_builder(Command::Start, true, true, true), result: None },
                Step { name: "stop txids 1", mutation: command_builder(Command::Stop, false, true, false), result: None },
                Step {
                    name: "stop txids 2",
                    mutation: command_builder(Command::Stop, false, true, false),
                    result: Some(command_builder(Command::Stop, false, true, false)),
                },
                Step { name: "stop blue 1", mutation: command_builder(Command::Stop, false, false, true), result: None },
                Step {
                    name: "stop blue 2",
                    mutation: command_builder(Command::Stop, false, false, true),
                    result: Some(command_builder(Command::Stop, false, false, true)),
                },
                Step { name: "stop active 1", mutation: command_builder(Command::Stop, true, false, false), result: None },
                Step {
                    name: "stop active 2",
                    mutation: command_builder(Command::Stop, true, false, false),
                    result: Some(command_builder(Command::Stop, true, false, false)),
                },
            ],
            final_state: none(),
        };
        let mut state = test.run();

        // Removing once more must panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            state.compound(command_builder(Command::Stop, true, false, false), &test.context)
        }));
        assert!(result.is_err(), "{}: trying to remove all when counter is zero must panic", test.name);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            state.compound(command_builder(Command::Stop, false, true, false), &test.context)
        }));
        assert!(result.is_err(), "{}: trying to remove reduced when counter is zero must panic", test.name);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            state.compound(command_builder(Command::Stop, false, false, true), &test.context)
        }));
        assert!(result.is_err(), "{}: trying to remove reduced when counter is zero must panic", test.name);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_utxos_changed_compounding() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let a_stock = get_3_addresses(true);

        let a = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let m = |command: Command, indexes: &[usize]| -> Mutation {
            Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope::new(a(indexes))) }
        };
        let none = Box::<UtxosChangedSubscription>::default;

        let add_all = || m(Command::Start, &[]);
        let remove_all = || m(Command::Stop, &[]);
        let add_0 = || m(Command::Start, &[0]);
        let add_1 = || m(Command::Start, &[1]);
        let add_01 = || m(Command::Start, &[0, 1]);
        let remove_0 = || m(Command::Stop, &[0]);
        let remove_1 = || m(Command::Stop, &[1]);

        let test = Test {
            name: "UtxosChanged",
            context: SubscriptionContext::new(),
            initial_state: none(),
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
            final_state: Box::new(UtxosChangedSubscription {
                all: 0,
                indexes: Counters::with_counters(vec![
                    Counter { index: 0, count: 0, locked: true },
                    Counter { index: 1, count: 0, locked: false },
                ]),
            }),
        };
        let mut state = test.run();

        // Removing once more must panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_all(), &test.context)));
        assert!(result.is_err(), "{}: trying to remove all when counter is zero must panic", test.name);
        // let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_0(), &test.context)));
        // assert!(result.is_err(), "{}: trying to remove an address when its counter is zero must panic", test.name);
    }
}
