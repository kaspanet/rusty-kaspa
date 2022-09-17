use core::fmt::{self, Display};
use core::ops::{Add, Div, Mul, Sub};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct SignedInteger<T> {
    abs: T,
    negative: bool,
}

impl<T> From<T> for SignedInteger<T> {
    #[inline]
    fn from(u: T) -> Self {
        Self { abs: u, negative: false }
    }
}
impl<T: From<u64>> SignedInteger<T> {
    #[inline]
    pub fn positive_u64(u: u64) -> Self {
        Self { abs: T::from(u), negative: false }
    }
}

impl<T: Display> Display for SignedInteger<T> {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negative {
            write!(f, "-")?;
        }
        write!(f, "{}", self.abs)
    }
}

impl<T: Copy> SignedInteger<T> {
    #[inline]
    pub const fn abs(&self) -> T {
        self.abs
    }

    #[inline]
    pub const fn negative(&self) -> bool {
        self.negative
    }
}

impl<T: Sub<Output = T> + Add<Output = T> + Ord> Sub for SignedInteger<T> {
    type Output = Self;
    #[inline]
    #[track_caller]
    fn sub(self, other: Self) -> Self::Output {
        match (self.negative, other.negative) {
            (false, false) | (true, true) => {
                if self.abs < other.abs {
                    Self { negative: !self.negative, abs: other.abs - self.abs }
                } else {
                    Self { negative: self.negative, abs: self.abs - other.abs }
                }
            }
            (false, true) | (true, false) => Self { negative: self.negative, abs: self.abs + other.abs },
        }
    }
}

impl<T: Mul<Output = T>> Mul for SignedInteger<T> {
    type Output = Self;
    #[inline]
    #[track_caller]
    fn mul(self, rhs: Self) -> Self::Output {
        Self { negative: self.negative ^ rhs.negative, abs: self.abs * rhs.abs }
    }
}

impl<T: Div<Output = T>> Div for SignedInteger<T> {
    type Output = Self;
    #[inline]
    #[track_caller]
    fn div(self, rhs: Self) -> Self::Output {
        Self { negative: self.negative ^ rhs.negative, abs: self.abs / rhs.abs }
    }
}
