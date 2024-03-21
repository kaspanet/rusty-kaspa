use crate::{
    events::{EventArray, EventType},
    listener::ListenerId,
    subscription::{compounded, single, CompoundedSubscription, DynSubscription},
};
use std::sync::Arc;

pub struct ArrayBuilder {}

impl ArrayBuilder {
    pub fn single(listener_id: ListenerId, utxos_changed_capacity: Option<usize>) -> EventArray<DynSubscription> {
        EventArray::from_fn(|i| {
            let event_type = EventType::try_from(i).unwrap();
            let subscription: DynSubscription = match event_type {
                EventType::VirtualChainChanged => Arc::<single::VirtualChainChangedSubscription>::default(),
                EventType::UtxosChanged => Arc::new(single::UtxosChangedSubscription::with_capacity(
                    single::UtxosChangedState::None,
                    listener_id,
                    utxos_changed_capacity.unwrap_or_default(),
                )),
                _ => Arc::new(single::OverallSubscription::new(event_type, false)),
            };
            subscription
        })
    }

    pub fn compounded(utxos_changed_capacity: Option<usize>) -> EventArray<CompoundedSubscription> {
        EventArray::from_fn(|i| {
            let event_type = EventType::try_from(i).unwrap();
            let subscription: CompoundedSubscription = match event_type {
                EventType::VirtualChainChanged => Box::<compounded::VirtualChainChangedSubscription>::default(),
                EventType::UtxosChanged => {
                    Box::new(compounded::UtxosChangedSubscription::with_capacity(utxos_changed_capacity.unwrap_or_default()))
                }
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
        let single = ArrayBuilder::single(0, None);
        let compounded = ArrayBuilder::compounded(None);
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
