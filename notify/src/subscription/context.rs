use crate::address::tracker::AddressTracker;
use std::{ops::Deref, sync::Arc};

#[derive(Debug, Default)]
pub struct SubscriptionContextInner {
    pub address_tracker: AddressTracker,
}

impl SubscriptionContextInner {
    pub fn new() -> Self {
        let address_tracker = AddressTracker::new();
        Self { address_tracker }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SubscriptionContext {
    inner: Arc<SubscriptionContextInner>,
}

impl SubscriptionContext {
    pub fn new() -> Self {
        let inner = Arc::new(SubscriptionContextInner::new());
        Self { inner }
    }
}

impl Deref for SubscriptionContext {
    type Target = SubscriptionContextInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
