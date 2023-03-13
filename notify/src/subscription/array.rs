use super::{compounded, single, CompoundedSubscription, SingleSubscription};
use crate::events::{EventArray, EventType};

pub struct ArrayBuilder {}

impl ArrayBuilder {
    pub fn single() -> EventArray<SingleSubscription> {
        EventArray::from_fn(|i| {
            let event_type = EventType::try_from(i).unwrap();
            let subscription: SingleSubscription = match event_type {
                EventType::VirtualChainChanged => Box::<single::VirtualChainChangedSubscription>::default(),
                EventType::UtxosChanged => Box::<single::UtxosChangedSubscription>::default(),
                _ => Box::new(single::OverallSubscription::new(event_type, false)),
            };
            subscription
        })
    }

    pub fn compounded() -> EventArray<CompoundedSubscription> {
        EventArray::from_fn(|i| {
            let event_type = EventType::try_from(i).unwrap();
            let subscription: CompoundedSubscription = match event_type {
                EventType::VirtualChainChanged => Box::<compounded::VirtualChainChangedSubscription>::default(),
                EventType::UtxosChanged => Box::<compounded::UtxosChangedSubscription>::default(),
                _ => Box::new(compounded::OverallSubscription::new(event_type)),
            };
            subscription
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EVENT_TYPE_ARRAY;

    #[test]
    fn test_array_builder() {
        let single = ArrayBuilder::single();
        let compounded = ArrayBuilder::compounded();
        EVENT_TYPE_ARRAY.into_iter().for_each(|event| {
            assert_eq!(
                event,
                single[event].event_type(),
                "single subscription array item {:?} reports wrong event type {:?}",
                event,
                single[event].event_type()
            );
            assert_eq!(
                event,
                compounded[event].event_type(),
                "compounded subscription array item {:?} reports wrong event type {:?}",
                event,
                compounded[event].event_type()
            );
        });
    }
}
