pub trait VecExtensions<T> {
    /// Pushes the provided value to the container if the container is empty
    fn push_if_empty(self, value: T) -> Self;
}

impl<T> VecExtensions<T> for Vec<T> {
    fn push_if_empty(mut self, value: T) -> Self {
        if self.is_empty() {
            self.push(value);
        }
        self
    }
}
