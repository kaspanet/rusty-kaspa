use std::rc::Rc;
use std::sync::Arc;

/// Something that can be seen as an immutable slice
pub trait AsSlice {
    /// The element type of the slice view
    type Element;

    /// Returns the immutable slice view of `Self`
    fn as_slice(&self) -> &[Self::Element];
}

/// Something that can be seen as an mutable slice
pub trait AsMutSlice: AsSlice {
    /// Returns the mutable slice view of `Self`
    fn as_mut_slice(&mut self) -> &mut [Self::Element];
}

impl<S> AsSlice for &S
where
    S: ?Sized + AsSlice,
{
    type Element = S::Element;

    fn as_slice(&self) -> &[S::Element] {
        (**self).as_slice()
    }
}

impl<S> AsSlice for &mut S
where
    S: ?Sized + AsSlice,
{
    type Element = S::Element;

    fn as_slice(&self) -> &[S::Element] {
        (**self).as_slice()
    }
}

impl<S> AsMutSlice for &mut S
where
    S: ?Sized + AsMutSlice,
{
    fn as_mut_slice(&mut self) -> &mut [S::Element] {
        (**self).as_mut_slice()
    }
}

impl<T> AsSlice for [T] {
    type Element = T;

    fn as_slice(&self) -> &[T] {
        self
    }
}

impl<T> AsSlice for Vec<T> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_slice()
    }
}

impl<T> AsSlice for Arc<Vec<T>> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_ref().as_slice()
    }
}
impl<T> AsSlice for Rc<Vec<T>> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_ref().as_slice()
    }
}
impl<T> AsSlice for Box<Vec<T>> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_ref().as_slice()
    }
}

impl<T> AsSlice for Arc<[T]> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_ref().as_slice()
    }
}
impl<T> AsSlice for Rc<[T]> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_ref().as_slice()
    }
}
impl<T> AsSlice for Box<[T]> {
    type Element = T;

    fn as_slice(&self) -> &[Self::Element] {
        self.as_ref().as_slice()
    }
}

impl<T> AsMutSlice for [T] {
    fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }
}

impl<T, const N: usize> AsSlice for [T; N] {
    type Element = T;

    fn as_slice(&self) -> &[T] {
        self
    }
}

impl<T, const N: usize> AsMutSlice for [T; N] {
    fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }
}
