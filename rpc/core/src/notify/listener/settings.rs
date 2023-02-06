use crate::RpcAddressSet;
use crate::notify::{events::EventArray, listener::filters::UtxosChangedFilter};

#[derive(Default, Debug, Clone)]
pub struct ListnerSettings {
    pub active_events: EventArray<bool>,
    pub utxo_changed_filter: Option<UtxosChangedFilter>,
}

impl ListnerSettings {
    fn new(active_events: EventArray<bool>, utxo_changed_filter: Option<UtxosChangedFilter>) -> Self {
        Self { active_events, utxo_changed_filter }
    }

    /// Note: 
    /// 
    /// 1. creates a new filter if no filter is present
    /// 2. method returns a bool indicating if the filter was changed, or a new one established. 
    fn add_to_utxo_changed_filter(&self, address_set: RpcAddressSet) -> bool {
        match self.utxo_changed_filter {
            Some() => {
                
            }
            None => {
                self.utxo_changed_filter = Some(UtxosChangedFilter::new(address_set));
                return true
            }
        }
    }

    /// Note: 
    /// 
    /// method returns a bool indicating if the filter was changed. 
    fn remove_from_utxo_changed_filter(&self, address_set: RpcAddresses) -> bool {
        self.utxo_changed_filter
        .expect("expected that one cannot remove from all")
        .remove_addresses(address_set)
    }

    /// Note: 
    /// 1. please use remove_utxo_changed_filter
    /// method returns a bool indicating if the filter was changed. 
    fn remove_utxo_changed_filter(&self) -> bool {
        match self.utxo_changed_filter {
            Some(_) => {
                self.utxo_changed_filter = None;
                return true
            }
            None => {
                return false
            }
        }
    }

    /// Note: 
    /// 1. please use remove_utxo_changed_filter
    /// method returns a bool indicating if the filter was changed. 
    fn add_utxo_changed_filter(&self, address_set: RpcAddressSet) -> bool {
        match self.utxo_changed_filter {
            Some(_) => {
                self.utxo_changed_filter = None;
                return true
            }
            None => {
                return false
            }
        }
    }
}