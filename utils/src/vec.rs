pub trait VecExtensions<T> {
    /// Pushes the provided value to the container if the container is empty
    fn push_if_empty(self, value: T) -> Self;

    /// Inserts the provided `value` at `index` while swapping the item at index to the end of the container
    fn swap_insert(&mut self, index: usize, value: T);

    /// Chains two containers one after the other and returns the result. The method is identical
    /// to [`Vec<T>::append`] but can be used more ergonomically in a fluent calling fashion
    fn chain(self, other: Self) -> Self;
}

impl<T> VecExtensions<T> for Vec<T> {
    fn push_if_empty(mut self, value: T) -> Self {
        if self.is_empty() {
            self.push(value);
        }
        self
    }

    fn swap_insert(&mut self, index: usize, value: T) {
        self.push(value);
        let loc = self.len() - 1;
        self.swap(index, loc);
    }

    fn chain(mut self, mut other: Self) -> Self {
        self.append(&mut other);
        self
    }
}
