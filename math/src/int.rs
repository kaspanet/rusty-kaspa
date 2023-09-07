use core::fmt::{self, Display};
use core::ops::{Add, Div, Mul, Sub};

#[derive(Copy, Clone, Debug)]
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

impl<T: PartialEq + PartialEq<u64>> PartialEq for SignedInteger<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.abs == 0 && other.abs == 0 {
            // neg/pos zeros are considered equal
            return true;
        }
        self.negative == other.negative && self.abs == other.abs
    }
}

impl<T: PartialEq + PartialEq<u64>> Eq for SignedInteger<T> {}

impl<T: PartialOrd + PartialEq<u64>> PartialOrd for SignedInteger<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if self.abs == 0 && other.abs == 0 {
            // neg/pos zeros are considered equal
            return Some(std::cmp::Ordering::Equal);
        }
        match (self.negative, other.negative) {
            (false, false) => self.abs.partial_cmp(&other.abs),
            (true, true) => other.abs.partial_cmp(&self.abs),
            (true, false) => Some(std::cmp::Ordering::Less),
            (false, true) => Some(std::cmp::Ordering::Greater),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{int::SignedInteger, Uint192};

    fn from_u64(val: u64) -> SignedInteger<Uint192> {
        SignedInteger::from(Uint192::from_u64(val))
    }

    #[test]
    fn test_partial_eq() {
        assert_eq!(from_u64(0), SignedInteger::from(Uint192::ZERO));
        assert_eq!(from_u64(0), from_u64(10) - from_u64(10));
        assert_eq!(from_u64(0), from_u64(10) - from_u64(20) - from_u64(10) * (from_u64(0) - from_u64(1))); // 0 == 10 - 20 -(-10)
        assert_eq!(from_u64(0) - from_u64(1000), from_u64(0) - from_u64(1000)); // -1000 = -1000
        assert_eq!(from_u64(1000), from_u64(1000));
    }

    #[test]
    fn test_partial_cmp() {
        // Test cases related to 0 and equality
        assert!(from_u64(0) >= from_u64(10) - from_u64(20) - from_u64(10) * (from_u64(0) - from_u64(1))); // pos 0 >= neg 0
        assert!(from_u64(0) <= from_u64(10) - from_u64(20) - from_u64(10) * (from_u64(0) - from_u64(1))); // pos 0 <= neg 0

        // Test all possible neg/pos combinations
        assert!(from_u64(100) > from_u64(0) - from_u64(1000)); // pos > neg
        assert!(from_u64(0) - from_u64(100) < from_u64(10)); // neg < pos
        assert!(from_u64(0) - from_u64(1000) < from_u64(0) - from_u64(100)); // -1000 < -100
        assert!(from_u64(0) - from_u64(1000) <= from_u64(0) - from_u64(1000)); // -1000 <= -1000
        assert!(from_u64(0) - from_u64(1000) >= from_u64(0) - from_u64(1000)); // -1000 >= -1000
        assert!(from_u64(1000) > from_u64(100));
        assert!(from_u64(100) < from_u64(1000));
        assert!(from_u64(1000) >= from_u64(1000));
        assert!(from_u64(100) <= from_u64(100));
    }
}
