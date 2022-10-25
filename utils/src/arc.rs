use std::sync::Arc;

pub trait ArcExtensions<T> {
    fn unwrap_or_clone(self) -> T;
}

impl<T: Clone> ArcExtensions<T> for Arc<T> {
    fn unwrap_or_clone(self) -> T {
        // Copy of Arc::unwrap_or_clone from unstable rust
        Arc::try_unwrap(self).unwrap_or_else(|arc| (*arc).clone())
    }
}
