pub trait OptionExtensions<T> {
    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool;
}

impl<T> OptionExtensions<T> for Option<T> {
    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Some(v) => f(v),
            None => true,
        }
    }
}
