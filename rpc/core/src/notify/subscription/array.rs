use super::{compounded, single, CompoundedSubscription, SingleSubscription};
use crate::notify::events::{EventArray, EventType};

pub struct ArrayBuilder {}

impl ArrayBuilder {
    pub fn single() -> EventArray<SingleSubscription> {
        let mut array: EventArray<SingleSubscription> = EventArray::from_fn(|i| {
            let subscription = single::OverallSubscription::new(i.try_into().unwrap(), false);
            let single: SingleSubscription = Box::new(subscription);
            single
        });
        array[EventType::VirtualSelectedParentChainChanged] = Box::<single::VirtualSelectedParentChainChangedSubscription>::default();
        array[EventType::UtxosChanged] = Box::<single::UtxosChangedSubscription>::default();
        array
    }

    pub fn compounded() -> EventArray<CompoundedSubscription> {
        let mut array: EventArray<CompoundedSubscription> = EventArray::from_fn(|i| {
            let subscription = compounded::OverallSubscription::new(i.try_into().unwrap());
            let compounded: CompoundedSubscription = Box::new(subscription);
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
