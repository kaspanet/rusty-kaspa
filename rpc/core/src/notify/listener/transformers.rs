use crate::notfiy::listener::listener::Listener;
use std::sync::Arc;
use crate::{Notification, UtxoChangedNotificationTypeModification, NotificationReceiver, NotificationSender, NotificationType};

use super::listener::ListenerSenderSide;

// This trait holds all listener specific transformers.
/// 
/// Note:
/// 
/// 1) To add a transformer method all relevent settings and methods should be added to [`ListenerSettings`] first
/// 
///     1.1) Potential filters can be added to `rpc-core::notify::listener::filter`
/// 
/// 2) the input / output should always be of the format `Arc<Notification>` -> `RpcResult<Option<Arc<Notification>>>`,
/// 
///     2.1) where possible a transformer should always return the orginal Notfication, with the original [`Arc`], 
///          these are cases where no changes to the input notification are required.
/// 
///     2.2) A None value in the [`Option`] indicats that a notfication is nullified, and will not be passed to the listener's recveiver. 
/// 
///     2.3) For a potential future, an additional [`Result`] can be added, in cases in which the listener should be purged.  
trait ListenerTransformers {
    fn transform_utxo_changed_notification(&self, notification: Arc<Notification>) -> Option<Arc<Notification>>;
}

impl ListenerTransformers for ListenerSenderSide {
    fn transform_utxo_changed_notification(&self, notification: Arc<Notification>) -> Option<Arc<Notification>> {
        let inner_notification = cast_to_inner!(notification, Notification::UtxosChanged);
        match self.settings().utxo_changed_filter() {
            Some(filter) =>  {
                let (maybe_new_inner_notfication, is_new_ref) = filter.apply_filter(cast_to_inner);
                match maybe_new_inner_notfication  {
                    Some(maybe_new_inner_notfication) => {
                        if is_new_ref { //alas, we most create a new notification in a new arc.
                            return Some(Arc::new(Notification::UtxosChanged(filtered_notfication)));
                        }
                        return Some(notification); //we can pass the original notifcation
                    }
                    None => return None,
                }
            }
            None => return notfication, //we don't have a filter, i.e. it is unfiltered. 
        };
    }
}