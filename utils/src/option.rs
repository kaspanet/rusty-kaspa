pub trait OptionExtensions<T> {
    /// Substitute for unstable [Option<T>::is_non_or]
    fn is_none_or_ex(&self, f: impl FnOnce(&T) -> bool) -> bool;
}

impl<T> OptionExtensions<T> for Option<T> {
    fn is_none_or_ex(&self, f: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Some(v) => f(v),
            None => true,
        }
    }
}
