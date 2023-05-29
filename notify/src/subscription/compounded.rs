use super::{Compounded, Mutation, Subscription};
use crate::{
    events::EventType,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::Command,
};
use kaspa_addresses::Address;
use std::collections::{HashMap, HashSet};

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
    fn compound(&mut self, mutation: Mutation) -> Option<Mutation> {
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

    fn scope(&self) -> Scope {
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
    fn compound(&mut self, mutation: Mutation) -> Option<Mutation> {
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

    fn scope(&self) -> Scope {
        Scope::VirtualChainChanged(VirtualChainChangedScope::new(self.all() > 0))
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct UtxosChangedSubscription {
    all: usize,
    addresses: HashMap<Address, usize>,
}

impl Compounded for UtxosChangedSubscription {
    fn compound(&mut self, mutation: Mutation) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::UtxosChanged(mut scope) = mutation.scope {
            match mutation.command {
                Command::Start => {
                    if scope.addresses.is_empty() {
                        // Add All
                        self.all += 1;
                        if self.all == 1 {
                            return Some(Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::default())));
                        }
                    } else {
                        // Add(A)
                        let mut added = vec![];
                        // Make sure no duplicate exists in addresses
                        let addresses: HashSet<Address> = scope.addresses.drain(0..).collect();
                        for address in addresses {
                            self.addresses.entry(address.clone()).and_modify(|counter| *counter += 1).or_insert_with(|| {
                                added.push(address);
                                1
                            });
                        }
                        if !added.is_empty() && self.all == 0 {
                            return Some(Mutation::new(Command::Start, Scope::UtxosChanged(UtxosChangedScope::new(added))));
                        }
                    }
                }
                Command::Stop => {
                    if !scope.addresses.is_empty() {
                        // Remove(R)
                        let mut removed = vec![];
                        // Make sure no duplicate exists in addresses
                        let addresses: HashSet<Address> = scope.addresses.drain(0..).collect();
                        for address in addresses {
                            assert!(self.addresses.contains_key(&address));
                            self.addresses.entry(address.clone()).and_modify(|counter| {
                                *counter -= 1;
                                if *counter == 0 {
                                    removed.push(address);
                                }
                            });
                        }
                        // Cleanup self.addresses
                        removed.iter().for_each(|x| {
                            self.addresses.remove(x);
                        });
                        if !removed.is_empty() && self.all == 0 {
                            return Some(Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed))));
                        }
                    } else {
                        // Remove All
                        assert!(self.all > 0);
                        self.all -= 1;
                        if self.all == 0 {
                            if !self.addresses.is_empty() {
                                return Some(Mutation::new(
                                    Command::Start,
                                    Scope::UtxosChanged(UtxosChangedScope::new(self.addresses.keys().cloned().collect())),
                                ));
                            } else {
                                return Some(Mutation::new(Command::Stop, Scope::UtxosChanged(UtxosChangedScope::default())));
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
        self.all > 0 || !self.addresses.is_empty()
    }

    fn scope(&self) -> Scope {
        let addresses = if self.all > 0 { vec![] } else { self.addresses.keys().cloned().collect() };
        Scope::UtxosChanged(UtxosChangedScope::new(addresses))
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use crate::{address::test_helpers::get_3_addresses, scope::BlockAddedScope};
    use std::panic::AssertUnwindSafe;

    struct Step {
        name: &'static str,
        mutation: Mutation,
        result: Option<Mutation>,
    }

    struct Test {
        name: &'static str,
        initial_state: CompoundedSubscription,
        steps: Vec<Step>,
        final_state: CompoundedSubscription,
    }

    impl Test {
        fn run(&self) -> CompoundedSubscription {
            let mut state = self.initial_state.clone_box();
            for step in self.steps.iter() {
                let result = state.compound(step.mutation.clone());
                assert_eq!(step.result, result, "{} - {}: wrong compound result", self.name, step.name);
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
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove())));
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
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_all())));
        assert!(result.is_err(), "{}: trying to remove all when counter is zero must panic", test.name);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_reduced())));
        assert!(result.is_err(), "{}: trying to remove reduced when counter is zero must panic", test.name);
    }

    #[test]
    #[allow(clippy::redundant_clone)]
    fn test_utxos_changed_compounding() {
        let a_stock = get_3_addresses(true);

        let a = |indexes: &[usize]| indexes.iter().map(|idx| (a_stock[*idx]).clone()).collect::<Vec<_>>();
        let m = |command: Command, indexes: &[usize]| -> Mutation {
            Mutation { command, scope: Scope::UtxosChanged(UtxosChangedScope { addresses: a(indexes) }) }
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
            final_state: none(),
        };
        let mut state = test.run();

        // Removing once more must panic
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_all())));
        assert!(result.is_err(), "{}: trying to remove all when counter is zero must panic", test.name);
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| state.compound(remove_0())));
        assert!(result.is_err(), "{}: trying to remove an address when its counter is zero must panic", test.name);
    }
}
