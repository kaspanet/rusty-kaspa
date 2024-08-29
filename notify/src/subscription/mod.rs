use crate::{error::Result, events::EventType, notification::Notification, scope::Scope, subscription::context::SubscriptionContext};
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::ops::Deref;
use std::{
    any::Any,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub mod array;
pub mod compounded;
pub mod context;
pub mod single;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum Command {
    Start = 0,
    Stop = 1,
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Command::Start => "start",
            Command::Stop => "stop",
        };
        write!(f, "{label}")
    }
}

impl From<Command> for i32 {
    fn from(item: Command) -> Self {
        item as i32
    }
}

impl From<i32> for Command {
    // We make this conversion infallible by falling back to Start from any unexpected value.
    fn from(item: i32) -> Self {
        if item == 1 {
            Command::Stop
        } else {
            Command::Start
        }
    }
}

/// Defines how an incoming UtxosChanged mutation must be propagated upwards
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum UtxosChangedMutationPolicy {
    /// Mutation granularity defined at address level
    #[default]
    AddressSet,

    /// Mutation granularity reduced to all or nothing
    Wildcard,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct MutationPolicies {
    pub utxo_changed: UtxosChangedMutationPolicy,
}

impl MutationPolicies {
    pub fn new(utxo_changed: UtxosChangedMutationPolicy) -> Self {
        Self { utxo_changed }
    }
}

/// A subscription mutation formed by a start/stop command and
/// a notification scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mutation {
    pub command: Command,
    pub scope: Scope,
}

impl Mutation {
    pub fn new(command: Command, scope: Scope) -> Self {
        Self { command, scope }
    }

    #[inline(always)]
    pub fn active(&self) -> bool {
        self.command == Command::Start
    }

    #[inline(always)]
    pub fn event_type(&self) -> EventType {
        (&self.scope).into()
    }
}

pub trait Subscription {
    fn event_type(&self) -> EventType;
    fn active(&self) -> bool;
    fn scope(&self, context: &SubscriptionContext) -> Scope;
}

pub trait Compounded: Subscription + AsAny + DynEq + CompoundedClone + Debug + Send + Sync {
    fn compound(&mut self, mutation: Mutation, context: &SubscriptionContext) -> Option<Mutation>;
}

impl PartialEq for dyn Compounded {
    fn eq(&self, other: &dyn Compounded) -> bool {
        DynEq::dyn_eq(self, other.as_any())
    }
}
impl Eq for dyn Compounded {}

pub type CompoundedSubscription = Box<dyn Compounded>;

/// The result of applying a [`Mutation`] to a [`DynSubscription`]
pub struct MutationOutcome {
    /// Optional new mutated subscription state
    pub mutated: Option<DynSubscription>,

    /// Mutations applied to the [`DynSubscription`]
    pub mutations: Vec<Mutation>,
}

impl MutationOutcome {
    pub fn new() -> Self {
        Self { mutated: None, mutations: vec![] }
    }

    pub fn with_mutations(mutations: Vec<Mutation>) -> Self {
        Self { mutated: None, mutations }
    }

    pub fn with_mutated(mutated: DynSubscription, mutations: Vec<Mutation>) -> Self {
        Self { mutated: Some(mutated), mutations }
    }

    /// Updates `target` to the mutated state if any, otherwise leave `target` as is.
    pub fn apply_to(self, target: &mut DynSubscription) -> Self {
        if let Some(ref mutated) = self.mutated {
            *target = mutated.clone();
        }
        self
    }

    #[inline(always)]
    pub fn has_new_state(&self) -> bool {
        self.mutated.is_some()
    }

    #[inline(always)]
    pub fn has_changes(&self) -> bool {
        self.has_new_state() || !self.mutations.is_empty()
    }
}

impl Default for MutationOutcome {
    fn default() -> Self {
        Self::new()
    }
}

/// A single subscription (as opposed to a compounded one)
pub trait Single: Subscription + AsAny + DynHash + DynEq + Debug + Send + Sync {
    /// Applies a [`Mutation`] to a single subscription.
    ///
    /// On success, returns both an optional new state and the mutations, if any, resulting of the process.
    ///
    /// Implementation guidelines:
    ///
    /// - If the processing of the mutation yields no change, the returned outcome must have no new state and no mutations
    ///   otherwise the outcome should contain both a new state (see next point for exception) and some mutations.
    /// - If the subscription has inner mutability and its current state and incoming mutation do allow an inner mutation,
    ///   the outcome new state must be empty.
    fn apply_mutation(
        &self,
        arc_self: &Arc<dyn Single>,
        mutation: Mutation,
        policies: MutationPolicies,
        context: &SubscriptionContext,
    ) -> Result<MutationOutcome>;
}

pub trait MutateSingle: Deref<Target = dyn Single> {
    /// Applies a [`Mutation`] to a single subscription.
    ///
    /// On success, updates `self` to the new state if any and returns both the optional new state and the mutations
    /// resulting of the process as a [`MutationOutcome`].
    fn mutate(&mut self, mutation: Mutation, policies: MutationPolicies, context: &SubscriptionContext) -> Result<MutationOutcome>;
}

impl MutateSingle for Arc<dyn Single> {
    fn mutate(&mut self, mutation: Mutation, policies: MutationPolicies, context: &SubscriptionContext) -> Result<MutationOutcome> {
        let outcome = self.apply_mutation(self, mutation, policies, context)?.apply_to(self);
        Ok(outcome)
    }
}

pub trait BroadcastingSingle: Deref<Target = dyn Single> {
    /// Returns the broadcasting instance of the subscription.
    ///
    /// This is used for grouping all the wildcard UtxosChanged subscriptions under
    /// the same unique instance in the broadcaster plans, allowing message optimizations
    /// during broadcasting of the notifications.
    fn broadcasting(self, context: &SubscriptionContext) -> DynSubscription;
}

impl Hash for dyn Single {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.dyn_hash(state);
    }
}
impl PartialEq for dyn Single {
    fn eq(&self, other: &dyn Single) -> bool {
        DynEq::dyn_eq(self, other.as_any())
    }
}
impl Eq for dyn Single {}

pub type DynSubscription = Arc<dyn Single>;

pub trait AsAny {
    fn as_any(&self) -> &dyn Any;
}
impl<T: Any> AsAny for T {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

pub trait DynHash {
    fn dyn_hash(&self, state: &mut dyn Hasher);
}
impl<H: Hash + ?Sized> DynHash for H {
    fn dyn_hash(&self, mut state: &mut dyn Hasher) {
        self.hash(&mut state);
    }
}

pub trait DynEq {
    fn dyn_eq(&self, other: &dyn Any) -> bool;
}
impl<T: Eq + Any> DynEq for T {
    fn dyn_eq(&self, other: &dyn Any) -> bool {
        if let Some(other) = other.downcast_ref::<Self>() {
            self == other
        } else {
            false
        }
    }
}

pub trait CompoundedClone {
    fn clone_box(&self) -> Box<dyn Compounded>;
}

impl<T> CompoundedClone for T
where
    T: 'static + Compounded + Clone,
{
    fn clone_box(&self) -> Box<dyn Compounded> {
        Box::new(self.clone())
    }
}

pub trait ApplyTo {
    fn apply_to<N: Notification>(&self, notification: &N) -> Option<N>;
}
