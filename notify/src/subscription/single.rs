use crate::{
    address::tracker::{Index, Indexes},
    error::Result,
    events::EventType,
    listener::ListenerId,
    scope::{Scope, UtxosChangedScope, VirtualChainChangedScope},
    subscription::{
        BroadcastingSingle, Command, DynSubscription, Mutation, MutationOutcome, MutationPolicies, Single, Subscription,
        UtxosChangedMutationPolicy, context::SubscriptionContext,
    },
};
use itertools::Itertools;
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_core::trace;
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::{
    collections::hash_set,
    fmt::{Debug, Display},
    hash::{Hash, Hasher},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
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
    fn apply_mutation(
        &self,
        _: &Arc<dyn Single>,
        mutation: Mutation,
        _: MutationPolicies,
        _: &SubscriptionContext,
    ) -> Result<MutationOutcome> {
        assert_eq!(self.event_type(), mutation.event_type());
        Ok(if self.active != mutation.active() {
            let mutated = Self::new(self.event_type, mutation.active());
            MutationOutcome::with_mutated(Arc::new(mutated), vec![mutation])
        } else {
            MutationOutcome::new()
        })
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

    fn scope(&self, _context: &SubscriptionContext) -> Scope {
        self.event_type.into()
    }
}

/// Subscription to VirtualChainChanged notifications
#[derive(Eq, PartialEq, Hash, Clone, Debug, Default)]
pub struct VirtualChainChangedSubscription {
    active: bool,
    include_accepted_transaction_ids: bool,
    include_accepting_blue_scores: bool,
}

impl VirtualChainChangedSubscription {
    pub fn new(active: bool, include_accepted_transaction_ids: bool, include_accepting_blue_scores: bool) -> Self {
        Self { active, include_accepted_transaction_ids, include_accepting_blue_scores }
    }
    pub fn include_accepted_transaction_ids(&self) -> bool {
        self.include_accepted_transaction_ids
    }
    pub fn include_accepting_blue_scores(&self) -> bool {
        self.include_accepting_blue_scores
    }
    pub fn active(&self) -> bool {
        self.active
    }
}

impl Single for VirtualChainChangedSubscription {
    fn apply_mutation(
        &self,
        _: &Arc<dyn Single>,
        mutation: Mutation,
        _: MutationPolicies,
        _: &SubscriptionContext,
    ) -> Result<MutationOutcome> {
        assert_eq!(self.event_type(), mutation.event_type());
        if let Scope::VirtualChainChanged(ref scope) = mutation.scope {
            // If this is a Start command
            match mutation.command {
                Command::Start => {
                    // check diff
                    let current_active = self.active;
                    let mutation_active = scope.active;
                    let current_accept = self.include_accepted_transaction_ids;
                    let current_blue = self.include_accepting_blue_scores;
                    let target_accept = scope.include_accepted_transaction_ids;
                    let target_blue = scope.include_accepting_blue_scores;
                    // No invariant assumed for Start mutations; accept/blue may be set independently of active.

                    let signal_accept = if !current_accept && target_accept { target_accept } else { false };
                    let signal_blue = if !current_blue && target_blue { target_blue } else { false };
                    let signal_active = if !current_active && mutation_active { mutation_active } else { false };

                    if signal_accept || signal_blue || signal_active {
                        // new state is the union of current and target (apply the start)
                        let new_state = Arc::new(VirtualChainChangedSubscription::new(
                            current_active || signal_active,
                            current_accept || signal_accept,
                            current_blue || signal_blue,
                        ));
                        let start_target = Mutation::new(
                            Command::Start,
                            VirtualChainChangedScope::new(signal_active, signal_accept, signal_blue).into(),
                        );
                        return Ok(MutationOutcome::with_mutated(new_state, vec![start_target]));
                    } else {
                        // No change
                        return Ok(MutationOutcome::new());
                    }
                }
                Command::Stop => {
                    // check diff
                    let current_active = self.active;
                    let mutation_active = scope.active;
                    let current_accept = self.include_accepted_transaction_ids;
                    let current_blue = self.include_accepting_blue_scores;
                    let target_accept = scope.include_accepted_transaction_ids;
                    let target_blue = scope.include_accepting_blue_scores;

                    // No invariant assumed for Stop mutations; accept/blue may be set independently of active.

                    let designal_accept = if current_accept && target_accept { target_accept } else { false };
                    let designal_blue = if current_blue && target_blue { target_blue } else { false };
                    let designal_active = if current_active && mutation_active { mutation_active } else { false };

                    if designal_accept || designal_blue || designal_active {
                        // new state is the current state with the target flags removed (apply the stop)
                        let new_state = Arc::new(VirtualChainChangedSubscription::new(
                            if designal_active { false } else { current_active },
                            if designal_accept { false } else { current_accept },
                            if designal_blue { false } else { current_blue },
                        ));
                        let start_target = Mutation::new(
                            Command::Stop,
                            VirtualChainChangedScope::new(designal_active, designal_accept, designal_blue).into(),
                        );
                        return Ok(MutationOutcome::with_mutated(new_state, vec![start_target]));
                    } else {
                        // No change
                        return Ok(MutationOutcome::new());
                    }
                }
            }
        };
        Ok(MutationOutcome::new())
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

    fn scope(&self, _context: &SubscriptionContext) -> Scope {
        VirtualChainChangedScope::new(self.active, self.include_accepted_transaction_ids, self.include_accepting_blue_scores).into()
    }
}

static UTXOS_CHANGED_SUBSCRIPTIONS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UtxosChangedMutation {
    None,
    Remove,
    Add,
    All,
}

impl From<(Command, &UtxosChangedScope)> for UtxosChangedMutation {
    fn from((command, scope): (Command, &UtxosChangedScope)) -> Self {
        match (command, scope.addresses.is_empty()) {
            (Command::Stop, true) => Self::None,
            (Command::Stop, false) => Self::Remove,
            (Command::Start, false) => Self::Add,
            (Command::Start, true) => Self::All,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub enum UtxosChangedState {
    /// Inactive
    #[default]
    None,

    /// Active on a set of selected addresses
    Selected,

    /// Active on all addresses
    All,
}

impl UtxosChangedState {
    pub fn active(&self) -> bool {
        match self {
            UtxosChangedState::None => false,
            UtxosChangedState::Selected | UtxosChangedState::All => true,
        }
    }
}

impl Display for UtxosChangedState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UtxosChangedState::None => write!(f, "none"),
            UtxosChangedState::Selected => write!(f, "selected"),
            UtxosChangedState::All => write!(f, "all"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct UtxosChangedSubscriptionData {
    /// State of the subscription
    ///
    /// Can be mutated without affecting neither equality nor hash of the struct
    state: UtxosChangedState,

    /// Address indexes in `SubscriptionContext`
    ///
    /// Can be mutated without affecting neither equality nor hash of the struct
    indexes: Indexes,
}

impl UtxosChangedSubscriptionData {
    fn with_capacity(state: UtxosChangedState, capacity: usize) -> Self {
        let indexes = Indexes::with_capacity(capacity);
        Self { state, indexes }
    }

    #[inline(always)]
    pub fn update_state(&mut self, new_state: UtxosChangedState) {
        self.state = new_state;
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

    pub fn capacity(&self) -> usize {
        self.indexes.capacity()
    }

    pub fn iter(&self) -> hash_set::Iter<'_, Index> {
        self.indexes.iter()
    }

    pub fn contains_address(&self, address: &Address, context: &SubscriptionContext) -> bool {
        context.address_tracker.contains_address(&self.indexes, address)
    }

    pub fn to_addresses(&self, prefix: Prefix, context: &SubscriptionContext) -> Vec<Address> {
        self.indexes.iter().filter_map(|index| context.address_tracker.get_address_at_index(*index, prefix)).collect_vec()
    }

    pub fn register(&mut self, addresses: Vec<Address>, context: &SubscriptionContext) -> Result<Vec<Address>> {
        Ok(context.address_tracker.register(&mut self.indexes, addresses)?)
    }

    pub fn unregister(&mut self, addresses: Vec<Address>, context: &SubscriptionContext) -> Vec<Address> {
        context.address_tracker.unregister(&mut self.indexes, addresses)
    }

    pub fn unregister_indexes(&mut self, context: &SubscriptionContext) -> Vec<Address> {
        // TODO: consider using a provided prefix
        let removed = self.to_addresses(Prefix::Mainnet, context);
        context.address_tracker.unregister_indexes(&mut self.indexes);
        removed
    }

    pub fn to_all(&self) -> bool {
        matches!(self.state, UtxosChangedState::All)
    }
}

impl Display for UtxosChangedSubscriptionData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.state {
            UtxosChangedState::None | UtxosChangedState::All => write!(f, "{}", self.state),
            UtxosChangedState::Selected => write!(f, "{}({})", self.state, self.indexes.len()),
        }
    }
}

#[derive(Debug)]
pub struct UtxosChangedSubscription {
    /// Mutable inner data
    data: RwLock<UtxosChangedSubscriptionData>,

    /// ID of the listener owning this subscription
    ///
    /// This fully determines both equality and hash.
    listener_id: ListenerId,
}

impl UtxosChangedSubscription {
    pub fn new(state: UtxosChangedState, listener_id: ListenerId) -> Self {
        Self::with_capacity(state, listener_id, 0)
    }

    pub fn with_capacity(state: UtxosChangedState, listener_id: ListenerId, capacity: usize) -> Self {
        let data = RwLock::new(UtxosChangedSubscriptionData::with_capacity(state, capacity));
        let subscription = Self { data, listener_id };
        trace!(
            "UtxosChangedSubscription: {} in total (new {})",
            UTXOS_CHANGED_SUBSCRIPTIONS.fetch_add(1, Ordering::SeqCst) + 1,
            subscription
        );
        subscription
    }

    #[cfg(test)]
    pub fn with_addresses(active: bool, addresses: Vec<Address>, listener_id: ListenerId, context: &SubscriptionContext) -> Self {
        let state = match (active, addresses.is_empty()) {
            (false, _) => UtxosChangedState::None,
            (true, false) => UtxosChangedState::Selected,
            (true, true) => UtxosChangedState::All,
        };
        let subscription = Self::with_capacity(state, listener_id, addresses.len());
        let _ = subscription.data_mut().register(addresses, context);
        subscription
    }

    pub fn data(&self) -> RwLockReadGuard<'_, UtxosChangedSubscriptionData> {
        self.data.read()
    }

    pub fn data_mut(&self) -> RwLockWriteGuard<'_, UtxosChangedSubscriptionData> {
        self.data.write()
    }

    #[inline(always)]
    pub fn state(&self) -> UtxosChangedState {
        self.data().state
    }

    pub fn to_all(&self) -> bool {
        matches!(self.data().state, UtxosChangedState::All)
    }
}

impl Clone for UtxosChangedSubscription {
    fn clone(&self) -> Self {
        let subscription = Self { data: RwLock::new(self.data().clone()), listener_id: self.listener_id };
        trace!(
            "UtxosChangedSubscription: {} in total (clone {})",
            UTXOS_CHANGED_SUBSCRIPTIONS.fetch_add(1, Ordering::SeqCst) + 1,
            subscription
        );
        subscription
    }
}

impl Display for UtxosChangedSubscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.data())
    }
}

impl Drop for UtxosChangedSubscription {
    fn drop(&mut self) {
        // Safely decrement the counter without underflowing. If the counter is
        // already zero, leave it as zero and log accordingly.
        let prev = UTXOS_CHANGED_SUBSCRIPTIONS
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| Some(if v == 0 { 0 } else { v - 1 }))
            .unwrap_or(0);
        let remaining = prev.saturating_sub(1);
        trace!("UtxosChangedSubscription: {} in total (drop {})", remaining, self);
    }
}

impl PartialEq for UtxosChangedSubscription {
    /// Equality is specifically bound to the listener ID
    fn eq(&self, other: &Self) -> bool {
        self.listener_id == other.listener_id
    }
}
impl Eq for UtxosChangedSubscription {}

impl Hash for UtxosChangedSubscription {
    /// Hash is specifically bound to the listener ID
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.listener_id.hash(state);
    }
}

impl Single for UtxosChangedSubscription {
    fn apply_mutation(
        &self,
        current: &Arc<dyn Single>,
        mutation: Mutation,
        policies: MutationPolicies,
        context: &SubscriptionContext,
    ) -> Result<MutationOutcome> {
        assert_eq!(self.event_type(), mutation.event_type());
        let outcome = if let Scope::UtxosChanged(scope) = mutation.scope {
            let mut data = self.data_mut();
            let state = data.state;
            let mutation_type = UtxosChangedMutation::from((mutation.command, &scope));
            match (state, mutation_type) {
                (UtxosChangedState::None, UtxosChangedMutation::None | UtxosChangedMutation::Remove) => {
                    // State None + Mutations None or Remove(R) => No change
                    MutationOutcome::new()
                }
                (UtxosChangedState::None, UtxosChangedMutation::Add) => {
                    // State None + Mutation Add(A) => Mutated new state Selected(A)
                    let addresses = data.register(scope.addresses, context)?;
                    data.update_state(UtxosChangedState::Selected);
                    let mutations = match policies.utxo_changed {
                        UtxosChangedMutationPolicy::AddressSet => {
                            vec![Mutation::new(mutation.command, UtxosChangedScope::new(addresses).into())]
                        }
                        UtxosChangedMutationPolicy::Wildcard => {
                            vec![Mutation::new(mutation.command, UtxosChangedScope::default().into())]
                        }
                    };
                    MutationOutcome::with_mutated(current.clone(), mutations)
                }
                (UtxosChangedState::None, UtxosChangedMutation::All) => {
                    // State None + Mutation All => Mutated new state All
                    data.update_state(UtxosChangedState::All);
                    let mutations = vec![Mutation::new(mutation.command, UtxosChangedScope::default().into())];
                    MutationOutcome::with_mutated(current.clone(), mutations)
                }
                (UtxosChangedState::Selected, UtxosChangedMutation::None) => {
                    // State Selected(S) + Mutation None => Mutated new state None
                    data.update_state(UtxosChangedState::None);
                    let removed = data.unregister_indexes(context);
                    assert!(!removed.is_empty(), "state Selected implies a non empty address set");
                    let mutations = match policies.utxo_changed {
                        UtxosChangedMutationPolicy::AddressSet => {
                            vec![Mutation::new(Command::Stop, UtxosChangedScope::new(removed).into())]
                        }
                        UtxosChangedMutationPolicy::Wildcard => {
                            vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())]
                        }
                    };
                    MutationOutcome::with_mutated(current.clone(), mutations)
                }
                (UtxosChangedState::Selected, UtxosChangedMutation::Remove) => {
                    // State Selected(S) + Mutation Remove(R) => Mutated state Selected(S – R) or mutated new state None or no change
                    let removed = data.unregister(scope.addresses, context);
                    match (removed.is_empty(), data.indexes.is_empty()) {
                        (false, false) => {
                            let mutations = match policies.utxo_changed {
                                UtxosChangedMutationPolicy::AddressSet => {
                                    vec![Mutation::new(Command::Stop, UtxosChangedScope::new(removed).into())]
                                }
                                UtxosChangedMutationPolicy::Wildcard => vec![],
                            };
                            MutationOutcome::with_mutations(mutations)
                        }
                        (false, true) => {
                            data.update_state(UtxosChangedState::None);
                            let mutations = match policies.utxo_changed {
                                UtxosChangedMutationPolicy::AddressSet => {
                                    vec![Mutation::new(Command::Stop, UtxosChangedScope::new(removed).into())]
                                }
                                UtxosChangedMutationPolicy::Wildcard => {
                                    vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())]
                                }
                            };
                            MutationOutcome::with_mutated(current.clone(), mutations)
                        }
                        (true, _) => MutationOutcome::new(),
                    }
                }
                (UtxosChangedState::Selected, UtxosChangedMutation::Add) => {
                    // State Selected(S) + Mutation Add(A) => Mutated state Selected(A ∪ S)
                    let added = data.register(scope.addresses, context)?;
                    match added.is_empty() {
                        false => {
                            let mutations = match policies.utxo_changed {
                                UtxosChangedMutationPolicy::AddressSet => {
                                    vec![Mutation::new(Command::Start, UtxosChangedScope::new(added).into())]
                                }
                                UtxosChangedMutationPolicy::Wildcard => vec![],
                            };
                            MutationOutcome::with_mutations(mutations)
                        }
                        true => MutationOutcome::new(),
                    }
                }
                (UtxosChangedState::Selected, UtxosChangedMutation::All) => {
                    // State Selected(S) + Mutation All => Mutated new state All
                    let removed = data.unregister_indexes(context);
                    assert!(!removed.is_empty(), "state Selected implies a non empty address set");
                    data.update_state(UtxosChangedState::All);
                    let mutations = match policies.utxo_changed {
                        UtxosChangedMutationPolicy::AddressSet => vec![
                            Mutation::new(Command::Stop, UtxosChangedScope::new(removed).into()),
                            Mutation::new(Command::Start, UtxosChangedScope::default().into()),
                        ],
                        UtxosChangedMutationPolicy::Wildcard => vec![],
                    };
                    MutationOutcome::with_mutated(current.clone(), mutations)
                }
                (UtxosChangedState::All, UtxosChangedMutation::None) => {
                    // State All + Mutation None => Mutated new state None
                    data.update_state(UtxosChangedState::None);
                    let mutations = vec![Mutation::new(Command::Stop, UtxosChangedScope::default().into())];
                    MutationOutcome::with_mutated(current.clone(), mutations)
                }
                (UtxosChangedState::All, UtxosChangedMutation::Remove) => {
                    // State All + Mutation Remove(R) => No change
                    MutationOutcome::new()
                }
                (UtxosChangedState::All, UtxosChangedMutation::Add) => {
                    // State All + Mutation Add(A) => Mutated new state Selectee(A)
                    let added = data.register(scope.addresses, context)?;
                    data.update_state(UtxosChangedState::Selected);
                    let mutations = match policies.utxo_changed {
                        UtxosChangedMutationPolicy::AddressSet => vec![
                            Mutation::new(Command::Start, UtxosChangedScope::new(added).into()),
                            Mutation::new(Command::Stop, UtxosChangedScope::default().into()),
                        ],
                        UtxosChangedMutationPolicy::Wildcard => vec![],
                    };
                    MutationOutcome::with_mutated(current.clone(), mutations)
                }
                (UtxosChangedState::All, UtxosChangedMutation::All) => {
                    // State All <= Mutation All
                    MutationOutcome::new()
                }
            }
        } else {
            MutationOutcome::new()
        };
        Ok(outcome)
    }
}

impl Subscription for UtxosChangedSubscription {
    fn event_type(&self) -> EventType {
        EventType::UtxosChanged
    }

    fn active(&self) -> bool {
        self.state().active()
    }

    fn scope(&self, context: &SubscriptionContext) -> Scope {
        // TODO: consider using a provided prefix
        UtxosChangedScope::new(self.data().to_addresses(Prefix::Mainnet, context)).into()
    }
}

impl BroadcastingSingle for DynSubscription {
    fn broadcasting(self, context: &SubscriptionContext) -> DynSubscription {
        match self.event_type() {
            EventType::UtxosChanged => {
                let utxos_changed_subscription = self.as_any().downcast_ref::<UtxosChangedSubscription>().unwrap();
                match utxos_changed_subscription.to_all() {
                    true => context.utxos_changed_subscription_to_all.clone(),
                    false => self,
                }
            }
            _ => self,
        }
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
                    Arc::new(VirtualChainChangedSubscription::new(false, false, false)),
                    Arc::new(VirtualChainChangedSubscription::new(true, false, false)),
                    Arc::new(VirtualChainChangedSubscription::new(true, true, false)),
                    Arc::new(VirtualChainChangedSubscription::new(true, true, false)),
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
                    Arc::new(UtxosChangedSubscription::with_addresses(false, vec![], 0, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, addresses[0..2].to_vec(), 1, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, addresses[0..3].to_vec(), 2, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, sorted_addresses[0..3].to_vec(), 2, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, vec![], 3, &context)),
                    Arc::new(UtxosChangedSubscription::with_addresses(true, vec![], 4, &context)),
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
                    Comparison::new(4, 5, false),
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
        outcome: MutationOutcome,
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
                let outcome = new_state.mutate(test.mutation.clone(), Default::default(), context).unwrap();
                assert_eq!(test.new_state.active(), new_state.active(), "Testing '{}': wrong new state activity", test.name);
                assert_eq!(*test.new_state, *new_state, "Testing '{}': wrong new state", test.name);
                assert_eq!(test.outcome.has_new_state(), outcome.has_new_state(), "Testing '{}': wrong new state presence", test.name);
                assert_eq!(test.outcome.mutations, outcome.mutations, "Testing '{}': wrong mutations", test.name);
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
                outcome: MutationOutcome::with_mutated(all(), vec![start_all()]),
            },
            MutationTest {
                name: "OverallSubscription None to None",
                state: none(),
                mutation: stop_all(),
                new_state: none(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "OverallSubscription All to All",
                state: all(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "OverallSubscription All to None",
                state: all(),
                mutation: stop_all(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_all()]),
            },
        ]);
        tests.run(&context)
    }

    #[test]
    fn test_virtual_chain_changed_mutation() {
        let context = SubscriptionContext::new();

        fn s(active: bool, include_accepted_transaction_ids: bool, include_accepting_blue_scores: bool) -> DynSubscription {
            Arc::new(VirtualChainChangedSubscription { active, include_accepted_transaction_ids, include_accepting_blue_scores })
        }
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

        let none = || s(false, false, false);
        let all = || s(true, true, true);
        let active_with_blue = || s(true, false, true);
        let active_with_txids = || s(true, true, false);

        // Mutations
        let start_all = || m(Command::Start, true, true, true);
        let stop_all = || m(Command::Stop, true, true, true);
        let start_active_with_txids = || m(Command::Start, true, true, false);
        let stop_active_with_txids = || m(Command::Stop, true, true, false);
        let start_blue_only = || m(Command::Start, false, false, true);
        let start_txids_only = || m(Command::Start, false, true, false);
        let start_txids_with_blue = || m(Command::Start, false, true, true);
        let stop_txids_with_blue = || m(Command::Stop, false, true, true);
        let start_reduced = || m(Command::Start, true, false, false);
        let stop_reduced = || m(Command::Stop, true, false, false);
        let start_reduced_blue = || m(Command::Start, true, false, true);
        let stop_reduced_blue = || m(Command::Stop, true, false, true);

        // Canonical fixed test list derived from the dump output (representative per mutation + composites)
        // Additional constructors used by the fixed test list (match dump labels)
        let reduced = || s(true, false, false);
        let reduced_blue = || s(true, false, true);
        let blue_only = || s(false, false, true);
        let txids_only = || s(false, true, false);
        let txids_with_blue = || s(false, true, true);

        let tests = MutationTests::new(vec![
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_all",
                state: none(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::with_mutated(all(), vec![start_all()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_reduced",
                state: none(),
                mutation: start_reduced(),
                new_state: reduced(),
                outcome: MutationOutcome::with_mutated(reduced(), vec![start_reduced()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_reduced_blue",
                state: none(),
                mutation: start_reduced_blue(),
                new_state: active_with_blue(),
                outcome: MutationOutcome::with_mutated(active_with_blue(), vec![start_reduced_blue()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_active_with_txids",
                state: none(),
                mutation: start_active_with_txids(),
                new_state: active_with_txids(),
                outcome: MutationOutcome::with_mutated(active_with_txids(), vec![start_active_with_txids()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_blue_only",
                state: none(),
                mutation: start_blue_only(),
                new_state: blue_only(),
                outcome: MutationOutcome::with_mutated(blue_only(), vec![start_blue_only()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_txids_only",
                state: none(),
                mutation: start_txids_only(),
                new_state: txids_only(),
                outcome: MutationOutcome::with_mutated(txids_only(), vec![start_txids_only()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription none to start_txids_with_blue",
                state: none(),
                mutation: start_txids_with_blue(),
                new_state: txids_with_blue(),
                outcome: MutationOutcome::with_mutated(txids_with_blue(), vec![start_txids_with_blue()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription reduced to start_all",
                state: reduced(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::with_mutated(all(), vec![start_txids_with_blue()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription reduced to start_reduced_blue",
                state: reduced(),
                mutation: start_reduced_blue(),
                new_state: reduced_blue(),
                outcome: MutationOutcome::with_mutated(reduced_blue(), vec![start_blue_only()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription reduced to stop_all",
                state: reduced(),
                mutation: stop_all(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_reduced()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription reduced to stop_reduced",
                state: reduced(),
                mutation: stop_reduced(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_reduced()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription reduced_blue to stop_reduced_blue",
                state: reduced_blue(),
                mutation: stop_reduced_blue(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_reduced_blue()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription active_with_txids to stop_active_with_txids",
                state: active_with_txids(),
                mutation: stop_active_with_txids(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_active_with_txids()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription all to stop_all",
                state: all(),
                mutation: stop_all(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_all()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription blue_only to start_all",
                state: blue_only(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::with_mutated(all(), vec![start_active_with_txids()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription txids_only to start_all",
                state: txids_only(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::with_mutated(all(), vec![start_reduced_blue()]),
            },
            MutationTest {
                name: "VirtualChainChangedSubscription txids_with_blue to stop_txids_with_blue",
                state: txids_with_blue(),
                mutation: stop_txids_with_blue(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_txids_with_blue()]),
            },
            // Edge case: all->start_reduced: no state change (START won't clear flags)
            MutationTest {
                name: "VirtualChainChangedSubscription all to start_reduced",
                state: all(),
                mutation: start_reduced(),
                new_state: all(),
                outcome: MutationOutcome::new(),
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
            Arc::new(UtxosChangedSubscription::with_addresses(active, ah(indexes).to_vec(), MutationTests::LISTENER_ID, &context))
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
                outcome: MutationOutcome::with_mutated(all(), vec![start_all()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription None to Selected 0 (add set)",
                state: none(),
                mutation: start_0(),
                new_state: selected_0(),
                outcome: MutationOutcome::with_mutated(selected_0(), vec![start_0()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription None to None (stop set)",
                state: none(),
                mutation: stop_0(),
                new_state: none(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "UtxosChangedSubscription None to None (stop all)",
                state: none(),
                mutation: stop_all(),
                new_state: none(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to All (add all)",
                state: selected_01(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::with_mutated(all(), vec![stop_01(), start_all()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to 01 (add set with total intersection)",
                state: selected_01(),
                mutation: start_1(),
                new_state: selected_01(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 0 to 01 (add set with partial intersection)",
                state: selected_0(),
                mutation: start_01(),
                new_state: selected_01(),
                outcome: MutationOutcome::with_mutations(vec![start_1()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 2 to 012 (add set with no intersection)",
                state: selected_2(),
                mutation: start_01(),
                new_state: selected_012(),
                outcome: MutationOutcome::with_mutations(vec![start_01()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to None (remove superset)",
                state: selected_1(),
                mutation: stop_01(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_1()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 01 to None (remove set with total intersection)",
                state: selected_01(),
                mutation: stop_01(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_01()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 02 to 2 (remove set with partial intersection)",
                state: selected_02(),
                mutation: stop_01(),
                new_state: selected_2(),
                outcome: MutationOutcome::with_mutations(vec![stop_0()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription Selected 02 to 02 (remove set with no intersection)",
                state: selected_02(),
                mutation: stop_1(),
                new_state: selected_02(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "UtxosChangedSubscription All to All (add all)",
                state: all(),
                mutation: start_all(),
                new_state: all(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "UtxosChangedSubscription All to Selected 01 (add set)",
                state: all(),
                mutation: start_01(),
                new_state: selected_01(),
                outcome: MutationOutcome::with_mutated(selected_01(), vec![start_01(), stop_all()]),
            },
            MutationTest {
                name: "UtxosChangedSubscription All to All (remove set)",
                state: all(),
                mutation: stop_01(),
                new_state: all(),
                outcome: MutationOutcome::new(),
            },
            MutationTest {
                name: "UtxosChangedSubscription All to None (remove all)",
                state: all(),
                mutation: stop_all(),
                new_state: none(),
                outcome: MutationOutcome::with_mutated(none(), vec![stop_all()]),
            },
        ]);
        tests.run(&context)
    }
}
