pub trait OptionExtensions<T> {
    fn has_value_and(&self, f: impl FnOnce(&T) -> bool) -> bool;
    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool;
}

impl<T> OptionExtensions<T> for Option<T> {
    fn has_value_and(&self, f: impl FnOnce(&T) -> bool) -> bool {
        // Copy of Option::is_some_and from unstable rust
        matches!(self, Some(x) if f(x))
    }

    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Some(v) => f(v),
            None => true,
        }
    }
}
