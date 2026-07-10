#![allow(unused)]

macro_rules! try_opt {
    ($expr: expr) => {
        match $expr {
            Some(value) => value,
            None => return,
        }
    };
}

macro_rules! assert_same {
    ($left:expr, $right:expr, $($arg:tt)+) => {
        let right = $right.to_bytes_le();
        assert_eq!(&$left.to_le_bytes()[..right.len()], &right[..], $($arg)+)
    };
}

pub fn consume<const N: usize>(data: &mut &[u8]) -> Option<[u8; N]> {
    if data.len() < N {
        None
    } else {
        let ret = &data[..N];
        *data = &(*data)[N..];
        Some(ret.try_into().unwrap())
    }
}

/// num-bigint extended-gcd modular-inverse reference: `a^-1 (mod n)` when `gcd(a, n) == 1`.
pub fn bigint_mod_inv(a: num_bigint::BigUint, n: num_bigint::BigUint) -> Option<num_bigint::BigUint> {
    use num_bigint::BigInt;
    use num_integer::Integer;
    use num_traits::Signed;
    let a = BigInt::from(a);
    let n = BigInt::from(n);
    let e_gcd = a.extended_gcd(&n);
    // An inverse exists iff gcd(a, n) == 1
    if e_gcd.gcd != 1u64.into() {
        None
    } else if e_gcd.x.is_negative() {
        (e_gcd.x + n).try_into().ok()
    } else {
        e_gcd.x.try_into().ok()
    }
}

pub(crate) use {assert_same, try_opt};
