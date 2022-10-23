pub trait OptionExtensions<T> {
    fn has_value_and(&self, f: impl FnOnce(&T) -> bool) -> bool;
    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool;
}

impl<T> OptionExtensions<T> for Option<T> {
    fn has_value_and(&self, f: impl FnOnce(&T) -> bool) -> bool {
        matches!(self, Some(x) if f(x))
    }

    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Some(v) => f(v),
            None => true,
        }
    }
}
