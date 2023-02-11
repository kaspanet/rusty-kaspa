use super::{compounded, single, DynCompoundedSubscription, DynSingleSubscription};
use crate::notify::events::{EventArray, EventType};

pub struct ArrayBuilder {}

impl ArrayBuilder {
    pub fn single() -> EventArray<DynSingleSubscription> {
        let mut array: EventArray<DynSingleSubscription> = EventArray::from_fn(|i| {
            let subscription = single::OverallSubscription::new(i.try_into().unwrap(), false);
            let single: DynSingleSubscription = Box::new(subscription);
            single
        });
        array[EventType::VirtualSelectedParentChainChanged] = Box::<single::VirtualSelectedParentChainChangedSubscription>::default();
        array[EventType::UtxosChanged] = Box::<single::UtxosChangedSubscription>::default();
        array
    }

    pub fn compounded() -> EventArray<DynCompoundedSubscription> {
        let mut array: EventArray<DynCompoundedSubscription> = EventArray::from_fn(|i| {
            let subscription = compounded::OverallSubscription::new(i.try_into().unwrap());
            let compounded: DynCompoundedSubscription = Box::new(subscription);
            compounded
        });
        array[EventType::VirtualSelectedParentChainChanged] =
            Box::<compounded::VirtualSelectedParentChainChangedSubscription>::default();
        array[EventType::UtxosChanged] = Box::<compounded::UtxosChangedSubscription>::default();
        array
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::events::EVENT_TYPE_ARRAY;

    #[test]
    fn test_array_builder() {
        let single = ArrayBuilder::single();
        let compounded = ArrayBuilder::compounded();
        EVENT_TYPE_ARRAY.iter().for_each(|event| {
            assert_eq!(
                *event,
                single[*event].event_type(),
                "subscription array item {:?} reports wrong event type {:?}",
                *event,
                single[*event].event_type()
            );
            assert_eq!(
                *event,
                compounded[*event].event_type(),
                "subscription array item {:?} reports wrong event type {:?}",
                *event,
                compounded[*event].event_type()
            );
        });
    }
}
