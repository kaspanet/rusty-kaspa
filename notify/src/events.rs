use super::scope::Scope;
use std::ops::{Index, IndexMut};
use workflow_core::enums::usize_try_from;

macro_rules! event_type_enum {
    ($(#[$meta:meta])* $vis:vis enum $name:ident {
        $($(#[$variant_meta:meta])* $variant_name:ident $(= $val:expr)?,)*
    }) => {
        usize_try_from!{
            $(#[$meta])* $vis enum $name {
                $($(#[$variant_meta])* $variant_name $(= $val)?,)*
            }
        }
        impl std::convert::From<&Scope> for $name {
            fn from(value: &Scope) -> Self {
                match value {
                    $(Scope::$variant_name(_) => $name::$variant_name),*
                }
            }
        }
        pub const EVENT_TYPE_ARRAY: [EventType; EVENT_COUNT] = [
            $($name::$variant_name),*
        ];
    }
}

event_type_enum! {
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum EventType {
    BlockAdded = 0,
    VirtualChainChanged,
    FinalityConflict,
    FinalityConflictResolved,
    UtxosChanged,
    SinkBlueScoreChanged,
    VirtualDaaScoreChanged,
    PruningPointUtxoSetOverride,
    NewBlockTemplate,
    ChainAcceptanceDataPruned,
}
}

pub const EVENT_COUNT: usize = 10;

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

    pub fn iter(&self) -> EventArrayIterator<'_, T> {
        EventArrayIterator::new(self)
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

pub struct EventArrayIterator<'a, T> {
    array: &'a EventArray<T>,
    index: usize,
}

impl<'a, T> EventArrayIterator<'a, T> {
    fn new(array: &'a EventArray<T>) -> Self {
        Self { array, index: 0 }
    }
}

impl<'a, T> Iterator for EventArrayIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.index < EVENT_TYPE_ARRAY.len() {
            true => {
                self.index += 1;
                Some(&self.array[EVENT_TYPE_ARRAY[self.index - 1]])
            }
            false => None,
        }
    }
}

/// An event type array of on/off switches
pub type EventSwitches = EventArray<bool>;

impl From<&[EventType]> for EventSwitches {
    fn from(events: &[EventType]) -> Self {
        let mut switches = EventSwitches::default();
        events.iter().for_each(|x| switches[*x] = true);
        switches
    }
}
