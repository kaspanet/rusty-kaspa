use crate::RpcAddressSet;
use kaspa_utils::counter::AHAshCounter;

// Compounds hold the compounded state of all settings of a notifier's listener's [`ListenerSettings`], via utilization of counter-based mutations
// see: `utils:counter::Counter` for the appropriate helper counter. this is tailored to the needs of 0-count flushing, and retrial of 0-count changes as values are added / removed.
//
// It is the responsibilty of a notfier's dispatcher to hold the compounded states and keep track of changes in its child listeners.
// the dispatcher should then only ever subscribe, or unsubscribe, with relevent compounded state changes to its parent when the compounded state requires a new NotficationType's payload. 
//
// Note: This means the Compounded state represents a complete state for the notifer's direct decendents,
// in regards to all decendents beyond, it only holds the relevent state. 
//
// the use-case of this, is it apply pre-filtering in parent notfiers.  

/// [`UtxosChangedCounter`] holds count of the notifier's listeners compounded settings regarding the [`UtxosChangedNotification`]. 
#[derive(Default, Clone, Debug)]
pub struct UtxosChangedCompoundedState {
    all_addresses_listeners: usize,
    selected_addresses_listeners: AHashCounter<RpcAddress>
}

impl UtxosChangedCompoundCompoundedState  {
    pub fn new (all_addresses_listeners: usize, selected_addresses_listeners: RpcAddressSet ) -> Self {
        Self { all_addresses_listeners, selected_addresses_listeners }
    }
    pub fn add_selected_addresses(&self, address_set: RpcAddressSet) -> RpcAddressSet {
        self.selected_addresses_listeners.add(address_set)
    }

    pub fn subtract_selected_addresses(&self, address_set: RpcAddressSet) -> RpcAddressSet {
        self.selected_addresses_listeners.remove(address_set)
    }

    pub fn increment_all_addresses(&mut self) -> usize {
        self.all_addresses_listeners += 1;
        self.all_addresses_listeners
    }

    pub fn decrement_all_addresses(&mut self) -> usize {
        self.all_addresses_listeners -= 1;
        self.all_addresses_listeners
    }
}