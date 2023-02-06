use std::async_iter::from_iter;
use crate::UtxosChangedNotification;



/// [`UtxosChangedFilter`] implements a filter for a [`UtxosChangedNotification`] via supplied addresses.
/// 
/// Note: 
/// 1) This implementation is reference-based, as such it trys to return the orginal reference where applicable / possible.
/// 2) Semantics mirror those of Kaspas's Api, i.e. an empty address-set represents a full address-set.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct UtxosChangedFilter {
    address_set: RpcAddressSet //Note if address_set is empty, listener filters for None. 
}

impl UtxosChangedFilter {
    pub fn new(address_set: RpcAddressSet) {
        Self{ address_set }
    }

    /// [`UtxosChangedFilter::apply_filter`] filters a refernce to a notification of type [`UtxosChangedNotification`].
    /// 
    /// Note: 
    /// 
    /// 1. Where applicable / possible it trys to return the orginal reference, 
    /// if a new reference was established the bool returns true
    /// If the filter filters everything it returns [`None`].
    /// 
    /// 2. to change the filter please use the relevent higher level Api found in [`ListenerSettings`]
    /// as this Api deals with the special cases of creating and removing filters, needed for of listening to all addresses. 
    pub fn apply_filter(&self, notification: &UtxosChangedNotification) -> (Option<&UtxosChangedNotification>, bool) {

        let original_length = notification.added.len() + notification.removed.len();

        let new_notification = UtxosChangedNotification{
            added: *notification
                .added
                .iter()
                .map(move |element| {
                    if allowed_utxos.contains_key(element.address) {
                        element.clone()
                    }
                })
                .collect(),
            removed: *notification
                .removed
                .iter()
                .map(move |element| {
                    if allowed_utxos.contains_key(element.address) {
                        element.clone()
                    }
                })
                .collect(),
        };
        
        if new_notification.added.is_empty() && new_notification.removed.is_empty() {
            return (None, false)
        } else if original_length == new_notification.added.len() + new_notification.removed.len() { //in this case we filtered no elements.
            drop(new_notification);
            return (Some(notfication), false) //so we drop new and keep reference to original.
        }
       
        (Some(&new_notification), true)
    }

    pub fn add_addresses(&self, address_set: RpcAddressSet) -> bool { 
        original_len = address_set.len();
        self.address_set.extend(address_set);
        original_len != self.address_set.len()
    }

    pub fn remove_addresses(&self, address_set: RpcAddressSet) -> bool {

        //TODO: remove sanity check when rpc-core is stable / has testing framework
        assert(!address_set.is_empty());

        original_len = address_set.len();
        if self.address_set.len() >= address_set.len() {
            for address in address_set {
                self.address_set.remove(address);
            }
        } else {
            for address in self.address_set {
                if address_set.contains_key(address) {
                    self.address_set.remove(address);
            }
        }
        original_len != self.address_set

        // TODO: remove sanity check when rpc-core is stable / has testing framework
        // 
        // Below indicates the listener will essentially stop filtering utxos by removing all addresses. which is not the 
        // intended purpose of this method, although it may seem like it is.
        //
        // Potential confustion can occur because we follow kaspad's api in the notifier implementation, 
        // hence we represent the full address-set as an empty address set. 
        // following this, the full-set should not be definable via this implementation's removing semantics, 
        // which clashes with that of a HashSet, where an empty set is defined as empty, 
        // and a possible mutation using the underlying HashSet's removing semantics is possible.
        // 
        // Note:
        //
        // 1) If the intention was to filter out all, 
        // the listener should unsubscribe to the event type completly
        //
        // 2) If the intention was to not apply a filter,
        // the listner should re-subscribe to the event with an empty address set.  
        assert(!self.address_set.is_empty());
    }
    }
}



#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct UtxosChangedFilter {
    address_set: RpcAddressSet //Note if address_set is empty, listener filters for None. 
}

impl UtxosChangedFilter {
    pub fn new(address_set: RpcAddressSet) {
        Self{ address_set }
    }

    /// [`UtxosChangedFilter::apply_filter`] filters a refernce to a notification of type [`UtxosChangedNotification`].
    /// 
    /// Note: Where applicable / possible it trys to return the orginal reference, 
    /// If the filter filters everything it returns [`None`].
    pub fn apply_filter(&self, notification: &UtxosChangedNotification) -> (Option<&UtxosChangedNotification>) {
        
        if self.address_set.is_empty() {
            return Some(notification)
        }
        
        let original_length = notification.added.len() + notification.removed.len();

        let new_notification = UtxosChangedNotification{
            added: *notification
                .added
                .iter()
                .map(move |element| {
                    if allowed_utxos.contains_key(element.address) {
                        element.clone()
                    }
                })
                .collect(),
            removed: *notification
                .removed
                .iter()
                .map(move |element| {
                    if allowed_utxos.contains_key(element.address) {
                        element.clone()
                    }
                })
                .collect(),
        };
        
        if new_notification.added.is_empty() && new_notification.removed.is_empty() {
            return None
        } else if original_length == new_notification.added.len() + new_notification.removed.len() { //in this case we filtered no elements.
            drop(new_notification);
            return Some(notfication) //we keep reference to original, drop new, no need for a new notfication. 
        }
       
        Some(&new_notification)
    }

    pub fn add_addresses(&self, address_set: RpcAddressSet) {
        if address_set.is_empty() { // i.e. apply_filter, will not filter.
            self.address_set = RpcAddressSet::new();
        }
        self.address_set.extend(address_set);
    }

    pub fn remove_addresses(&self, address_set: RpcAddressSet) {
        if self.address_set.len() >= address_set.len() {
            for address in address_set {
                self.address_set.remove(address);
            }
        } else {
            for address in self.address_set {
                if address_set.contains_key(address) {
                    self.address_set.remove(address);
            }
        }

        // Sanity-check, until rpc-core is fully stable. Below should never happen!
        // 
        // It indicates the listener will stop filtering utxos by removing all addresses. which is not the 
        // intended purpose of this method!
        //
        // Potential confustion can occur because we represent all addresses as an empty address set. 
        // and this should not be definable via removing semantics!
        // 
        // 1) If the intention was to filter out all, 
        // the listener should unsubscribe to the event type completly
        //
        // 2) If the intention was to not apply a filter,
        // the listner should re-subscribe to the event with an empty address set.  
        if address_set.is_empty() {
            panic!("address_set in UtxosChangedFilter::remove_addresses is empty, This should never happen!");
        }
    }
    }
}
