use std::ops::{Index, IndexMut};

use crate::{Notification, NotificationType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
impl From<EventType> for NotificationType {
    fn from(item: EventType) -> Self {
        match item {
            EventType::BlockAdded => NotificationType::BlockAdded,
            EventType::VirtualSelectedParentChainChanged => NotificationType::VirtualSelectedParentChainChanged,
            EventType::FinalityConflict => NotificationType::FinalityConflict,
            EventType::FinalityConflictResolved => NotificationType::FinalityConflictResolved,
            EventType::UtxosChanged => NotificationType::UtxosChanged(vec![]),
            EventType::VirtualSelectedParentBlueScoreChanged => NotificationType::VirtualSelectedParentBlueScoreChanged,
            EventType::VirtualDaaScoreChanged => NotificationType::VirtualDaaScoreChanged,
            EventType::PruningPointUTXOSetOverride => NotificationType::PruningPointUtxoSetOverride,
            EventType::NewBlockTemplate => NotificationType::NewBlockTemplate,
        }
    }
}

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
impl From<&NotificationType> for EventType {
    fn from(item: &NotificationType) -> Self {
        match item {
            NotificationType::BlockAdded => EventType::BlockAdded,
            NotificationType::VirtualSelectedParentChainChanged => EventType::VirtualSelectedParentChainChanged,
            NotificationType::FinalityConflict => EventType::FinalityConflict,
            NotificationType::FinalityConflictResolved => EventType::FinalityConflictResolved,
            NotificationType::UtxosChanged(_) => EventType::UtxosChanged,
            NotificationType::VirtualSelectedParentBlueScoreChanged => EventType::VirtualSelectedParentBlueScoreChanged,
            NotificationType::VirtualDaaScoreChanged => EventType::VirtualDaaScoreChanged,
            NotificationType::PruningPointUtxoSetOverride => EventType::PruningPointUTXOSetOverride,
            NotificationType::NewBlockTemplate => EventType::NewBlockTemplate,
        }
    }
}

/// Generic array with [`EventType`] strongly-typed index
#[derive(Default, Clone, Copy, Debug)]
pub(crate) struct EventArray<T>([T; EVENT_COUNT]);

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
