use super::{super::scope::Scope, Compounded, Mutation, Subscription};
use crate::{
    api::ops::SubscribeCommand,
    notify::{
        events::EventType,
        scope::{UtxosChangedScope, VirtualSelectedParentChainChangedScope},
    },
};
use addresses::Address;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
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
            SubscribeCommand::Start => {
                self.active += 1;
                if self.active == 1 {
                    return Some(mutation);
                }
            }
            SubscribeCommand::Stop => {
                if self.active > 0 {
                    self.active -= 1;
                }
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
}

#[derive(Default, Debug)]
pub struct VirtualSelectedParentChainChangedSubscription {
    include_accepted_transaction_ids: [usize; 2],
}

impl VirtualSelectedParentChainChangedSubscription {
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

impl Compounded for VirtualSelectedParentChainChangedSubscription {
    fn compound(&mut self, mutation: Mutation) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::VirtualSelectedParentChainChanged(ref scope) = mutation.scope {
            let all = scope.include_accepted_transaction_ids;
            match mutation.command {
                SubscribeCommand::Start => {
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
                SubscribeCommand::Stop => {
                    if !all {
                        // Remove Reduced
                        if self.reduced() > 0 {
                            *self.reduced_mut() += 1;
                            if self.reduced() == 0 && self.all() == 0 {
                                return Some(mutation);
                            }
                        }
                    } else {
                        // Remove All
                        if self.all() > 0 {
                            *self.all_mut() -= 1;
                            if self.all() == 0 {
                                if self.reduced() > 0 {
                                    return Some(Mutation::new(
                                        SubscribeCommand::Start,
                                        Scope::VirtualSelectedParentChainChanged(VirtualSelectedParentChainChangedScope::new(false)),
                                    ));
                                } else {
                                    return Some(mutation);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

impl Subscription for VirtualSelectedParentChainChangedSubscription {
    #[inline(always)]
    fn event_type(&self) -> EventType {
        EventType::VirtualSelectedParentChainChanged
    }
}

#[derive(Default, Debug)]
pub struct UtxosChangedSubscription {
    all: usize,
    addresses: HashMap<Address, usize>,
}

impl Compounded for UtxosChangedSubscription {
    fn compound(&mut self, mutation: Mutation) -> Option<Mutation> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::UtxosChanged(mut scope) = mutation.scope {
            match mutation.command {
                SubscribeCommand::Start => {
                    if scope.addresses.is_empty() {
                        // Add All
                        self.all += 1;
                        if self.all == 1 {
                            return Some(Mutation::new(SubscribeCommand::Start, Scope::UtxosChanged(UtxosChangedScope::default())));
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
                            return Some(Mutation::new(SubscribeCommand::Start, Scope::UtxosChanged(UtxosChangedScope::new(added))));
                        }
                    }
                }
                SubscribeCommand::Stop => {
                    if !scope.addresses.is_empty() {
                        // Remove Reduced
                        let mut removed = vec![];
                        // Make sure no duplicate exists in addresses
                        let addresses: HashSet<Address> = scope.addresses.drain(0..).collect();
                        for address in addresses {
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
                            return Some(Mutation::new(SubscribeCommand::Stop, Scope::UtxosChanged(UtxosChangedScope::new(removed))));
                        }
                    } else {
                        // Remove All
                        if self.all > 0 {
                            self.all -= 1;
                            if self.all == 0 {
                                if !self.addresses.is_empty() {
                                    return Some(Mutation::new(
                                        SubscribeCommand::Start,
                                        Scope::UtxosChanged(UtxosChangedScope::new(self.addresses.keys().cloned().collect())),
                                    ));
                                } else {
                                    return Some(Mutation::new(
                                        SubscribeCommand::Stop,
                                        Scope::UtxosChanged(UtxosChangedScope::default()),
                                    ));
                                }
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
}
