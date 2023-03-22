/// [`NotificationMode`] controls notification delivery process
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationMode {
    /// Local notifier is used for notification processing.
    ///
    /// Multiple listeners can register and subscribe independently.
    MultiListeners,
    /// No notifier is present, notifications are relayed
    /// directly through the internal channel to a single listener.
    Direct,
}
