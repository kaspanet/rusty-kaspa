pub trait OptionExtensions<T> {
    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool;
    fn is_some_perform(&self, f: impl FnOnce(&T));
}

impl<T> OptionExtensions<T> for Option<T> {
    fn is_none_or(&self, f: impl FnOnce(&T) -> bool) -> bool {
        match self {
            Some(v) => f(v),
            None => true,
        }
    }

    fn is_some_perform(&self, f: impl FnOnce(&T)) {
        if let Some(v) = self {
            f(v);
        }
    }
}
