use crate::{
    address::{error::Result, tracker::Counters},
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::{context::SubscriptionContext, Command, Compounded, Mutation, Subscription},
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
    // counts[include_accepted_transaction_ids as usize][include_accepting_blue_scores as usize]
    counts: [[usize; 2]; 2],
}

impl VirtualChainChangedSubscription {
    #[inline(always)]
    fn idx(b: bool) -> usize {
        b as usize
    }

    #[inline(always)]
    fn inc(&mut self, accepted_tx: bool, blue: bool) -> usize {
        let c = &mut self.counts[Self::idx(accepted_tx)][Self::idx(blue)];
        *c += 1;
        *c
    }

    #[inline(always)]
    fn dec(&mut self, accepted_tx: bool, blue: bool) -> usize {
        let c = &mut self.counts[Self::idx(accepted_tx)][Self::idx(blue)];
        assert!(*c > 0);
        *c -= 1;
        *c
    }

    #[inline(always)]
    fn any_all(&self) -> usize {
        self.counts[1][0] + self.counts[1][1]
    }

    #[inline(always)]
    fn any_reduced(&self) -> usize {
        self.counts[0][0] + self.counts[0][1]
    }

    #[inline(always)]
    fn any_all_blue(&self) -> usize {
        self.counts[1][1]
    }

    #[inline(always)]
    fn any_reduced_blue(&self) -> usize {
        self.counts[0][1]
    }

    #[inline(always)]
    pub fn include_accepted_transaction_ids(&self) -> bool {
        self.any_all() > 0
    }

    #[inline(always)]
    pub fn include_accepting_blue_scores(&self) -> bool {
        (self.any_all_blue() + self.any_reduced_blue()) > 0
    }
}

impl Compounded for VirtualChainChangedSubscription {
    fn compound(&mut self, mutation: Mutation, _context: &SubscriptionContext) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::VirtualChainChanged(ref scope) = mutation.scope {
            let tx_all = scope.include_accepted_transaction_ids;
            let blue = scope.include_accepting_blue_scores;
            match mutation.command {
                Command::Start => {
                    let prev_any_all = self.any_all();
                    let prev_any_reduced = self.any_reduced();
                    let prev_all_blue = self.any_all_blue() > 0;
                    let prev_reduced_blue = self.any_reduced_blue() > 0;

                    self.inc(tx_all, blue);

                    // Start logic
                    if tx_all {
                        // Adding an All subscription
                        if prev_any_all == 0 {
                            // first all - reveal all (with aggregated blue)
                            return Some(Mutation::new(
                                Command::Start,
                                Scope::VirtualChainChanged(VirtualChainChangedScope::new(true, self.any_all_blue() > 0)),
                            ));
                        } else if !prev_all_blue && self.any_all_blue() > 0 {
                            // all existed but blue aggregated toggled to true
                            return Some(Mutation::new(
                                Command::Start,
                                Scope::VirtualChainChanged(VirtualChainChangedScope::new(true, true)),
                            ));
                        }
                    } else {
                        // Adding a Reduced subscription
                        if self.any_all() == 0 {
                            if prev_any_reduced == 0 {
                                // first reduced - reveal reduced (with aggregated blue)
                                return Some(Mutation::new(
                                    Command::Start,
                                    Scope::VirtualChainChanged(VirtualChainChangedScope::new(false, self.any_reduced_blue() > 0)),
                                ));
                            } else if !prev_reduced_blue && self.any_reduced_blue() > 0 {
                                // reduced existed but blue aggregated toggled to true
                                return Some(Mutation::new(
                                    Command::Start,
                                    Scope::VirtualChainChanged(VirtualChainChangedScope::new(false, true)),
                                ));
                            }
                        }
                    }
                }
                Command::Stop => {
                    let prev_any_all = self.any_all();
                    let prev_any_reduced = self.any_reduced();
                    let prev_all_blue = self.any_all_blue() > 0;
                    let prev_reduced_blue = self.any_reduced_blue() > 0;

                    if tx_all {
                        // Remove All
                        assert!(prev_any_all > 0);
                        self.dec(true, blue);
                        let new_any_all = self.any_all();

                        if new_any_all == 0 {
                            if self.any_reduced() > 0 {
                                // reveal reduced (aggregated blue)
                                return Some(Mutation::new(
                                    Command::Start,
                                    Scope::VirtualChainChanged(VirtualChainChangedScope::new(false, self.any_reduced_blue() > 0)),
                                ));
                            } else {
                                // stop all
                                return Some(Mutation::new(
                                    Command::Stop,
                                    Scope::VirtualChainChanged(VirtualChainChangedScope::new(true, prev_all_blue)),
                                ));
                            }
                        } else if prev_all_blue && self.any_all_blue() == 0 {
                            // blue toggled from true to false while all still present
                            return Some(Mutation::new(
                                Command::Stop,
                                Scope::VirtualChainChanged(VirtualChainChangedScope::new(true, true)),
                            ));
                        }
                    } else {
                        // Remove Reduced
                        assert!(prev_any_reduced > 0);
                        self.dec(false, blue);
                        let new_any_reduced = self.any_reduced();

                        if new_any_reduced == 0 && self.any_all() == 0 {
                            // no reduced remain and no all => stop reduced
                            return Some(Mutation::new(
                                Command::Stop,
                                Scope::VirtualChainChanged(VirtualChainChangedScope::new(false, prev_reduced_blue)),
                            ));
                        } else if prev_reduced_blue && self.any_reduced_blue() == 0 && self.any_all() == 0 {
                            // last reduced blue removed while all absent
                            return Some(Mutation::new(
                                Command::Stop,
                                Scope::VirtualChainChanged(VirtualChainChangedScope::new(false, true)),
                            ));
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
        self.any_all() + self.any_reduced() > 0
    }

    fn scope(&self, _context: &SubscriptionContext) -> Scope {
        Scope::VirtualChainChanged(VirtualChainChangedScope::new(
            self.include_accepted_transaction_ids(),
            self.include_accepting_blue_scores(),
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
        fn m(command: Command, include_accepted_transaction_ids: bool, include_accepting_blue_scores: bool) -> Mutation {
            Mutation {
                command,
                scope: Scope::VirtualChainChanged(VirtualChainChangedScope {
                    include_accepted_transaction_ids,
                    include_accepting_blue_scores,
                }),
            }
        }
        let none = Box::<VirtualChainChangedSubscription>::default;
        // default blue flag is false for legacy behavior in these unit tests
        let add_all = || m(Command::Start, true, false);
        let add_reduced = || m(Command::Start, false, false);
        let remove_reduced = || m(Command::Stop, false, false);
        let remove_all = || m(Command::Stop, true, false);
        let test = Test {
            name: "VirtualChainChanged",
            context: SubscriptionContext::new(),
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
