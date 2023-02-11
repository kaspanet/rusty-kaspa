use super::scope::Scope;
use crate::Notification;
use std::ops::{Index, IndexMut};
use workflow_core::enums::usize_try_from;

usize_try_from! {
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EventType {
    BlockAdded = 0,
    VirtualSelectedParentChainChanged,
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged,
    VirtualSelectedParentBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUTXOSetOverride,
    NewBlockTemplate,
}
}

// TODO: write a macro or use an external crate to get this
pub(crate) const EVENT_COUNT: usize = 9;

// TODO: write a macro or use an external crate to get this
pub const EVENT_TYPE_ARRAY: [EventType; EVENT_COUNT] = [
    EventType::BlockAdded,
    EventType::VirtualSelectedParentChainChanged,
    EventType::FinalityConflict,
    EventType::FinalityConflictResolved,
    EventType::UtxosChanged,
    EventType::VirtualSelectedParentBlueScoreChanged,
    EventType::VirtualDaaScoreChanged,
    EventType::PruningPointUTXOSetOverride,
    EventType::NewBlockTemplate,
];

// TODO: write a macro to get this
impl From<&Notification> for EventType {
    fn from(item: &Notification) -> Self {
        match item {
            Notification::BlockAdded(_) => EventType::BlockAdded,
            Notification::VirtualSelectedParentChainChanged(_) => EventType::VirtualSelectedParentChainChanged,
            Notification::FinalityConflict(_) => EventType::FinalityConflict,
            Notification::FinalityConflictResolved(_) => EventType::FinalityConflictResolved,
            Notification::UtxosChanged(_) => EventType::UtxosChanged,
            Notification::VirtualSelectedParentBlueScoreChanged(_) => EventType::VirtualSelectedParentBlueScoreChanged,
            Notification::VirtualDaaScoreChanged(_) => EventType::VirtualDaaScoreChanged,
            Notification::PruningPointUtxoSetOverride(_) => EventType::PruningPointUTXOSetOverride,
            Notification::NewBlockTemplate(_) => EventType::NewBlockTemplate,
        }
    }
}

// TODO: write a macro to get this
impl From<&Scope> for EventType {
    fn from(item: &Scope) -> Self {
        match item {
            Scope::BlockAdded => EventType::BlockAdded,
            Scope::VirtualSelectedParentChainChanged(_) => EventType::VirtualSelectedParentChainChanged,
            Scope::FinalityConflict => EventType::FinalityConflict,
            Scope::FinalityConflictResolved => EventType::FinalityConflictResolved,
            Scope::UtxosChanged(_) => EventType::UtxosChanged,
            Scope::VirtualSelectedParentBlueScoreChanged => EventType::VirtualSelectedParentBlueScoreChanged,
            Scope::VirtualDaaScoreChanged => EventType::VirtualDaaScoreChanged,
            Scope::PruningPointUtxoSetOverride => EventType::PruningPointUTXOSetOverride,
            Scope::NewBlockTemplate => EventType::NewBlockTemplate,
        }
    }
}

/// Generic array with [`EventType`] strongly-typed index
#[derive(Default, Clone, Copy, Debug)]
pub struct EventArray<T>([T; EVENT_COUNT]);

impl<T> EventArray<T> {
    pub fn from_fn<F>(cb: F) -> Self
    where
        F: FnMut(usize) -> T,
    {
        let array: [T; EVENT_COUNT] = core::array::from_fn(cb);
        Self(array)
    }
}

impl<T> Index<EventType> for EventArray<T> {
    type Output = T;

    fn index(&self, index: EventType) -> &Self::Output {
        let idx = index as usize;
        &self.0[idx]
    }
}

impl<T> IndexMut<EventType> for EventArray<T> {
    fn index_mut(&mut self, index: EventType) -> &mut Self::Output {
        let idx = index as usize;
        &mut self.0[idx]
    }
}
