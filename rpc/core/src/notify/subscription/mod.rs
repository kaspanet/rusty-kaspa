use super::{events::EventType, scope::Scope};
use crate::{api::ops::SubscribeCommand, Notification};
use std::{
    any::Any,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};

pub mod array;
pub mod compounded;
pub mod single;

/// A subscription mutation including a start/stop command and
/// a notification scope.
pub struct Mutation {
    command: SubscribeCommand,
    scope: Scope,
}

impl Mutation {
    pub fn new(command: SubscribeCommand, scope: Scope) -> Self {
        Self { command, scope }
    }

    #[inline(always)]
    pub fn active(&self) -> bool {
        self.command == SubscribeCommand::Start
    }

    #[inline(always)]
    pub fn event_type(&self) -> EventType {
        (&self.scope).into()
    }
}

pub trait Subscription {
    fn event_type(&self) -> EventType;
}

pub trait Compounded: Subscription {
    fn compound(&mut self, mutation: Mutation) -> Option<Mutation>;
}

pub type DynCompoundedSubscription = Box<dyn Compounded>;

pub trait Single: Subscription + AsAny + DynHash + DynEq + Debug + Send + Sync {
    fn active(&self) -> bool;
    fn apply_to(&self, notification: Arc<Notification>) -> Arc<Notification>;
    fn mutate(&mut self, mutation: Mutation) -> Option<Vec<Mutation>>;
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

pub type DynSingleSubscription = Box<dyn Single>;

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
