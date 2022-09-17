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

pub(crate) use {assert_same, try_opt};
