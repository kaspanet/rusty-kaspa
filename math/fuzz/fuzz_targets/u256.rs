#![no_main]
use core::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Rem};
use libfuzzer_sys::fuzz_target;
use math::construct_uint;
use num_bigint::BigUint;
use num_traits::Zero;
use std::convert::TryInto;

construct_uint!(Uint256, 4);

macro_rules! try_opt {
    ($expr: expr) => {
        match $expr {
            Some(value) => value,
            None => return,
        }
    };
}

fn consume<const N: usize>(data: &mut &[u8]) -> Option<[u8; N]> {
    if data.len() < N {
        None
    } else {
        let ret = &data[..N];
        *data = &(*data)[N..];
        Some(ret.try_into().unwrap())
    }
}

// Consumes 16 bytes
fn generate_ints(data: &mut &[u8]) -> Option<(Uint256, BigUint)> {
    let buf = consume(data)?;
    Some((Uint256::from_le_bytes(buf), BigUint::from_bytes_le(&buf)))
}

macro_rules! assert_same {
    ($left:expr, $right:expr, $($arg:tt)+) => {
        let right = $right.to_bytes_le();
        assert_eq!(&$left.to_le_bytes()[..right.len()], &right[..], $($arg)+)
    };
}

#[track_caller]
fn assert_op<T, U>(data: &mut &[u8], op_lib: T, op_native: U, ok_by_zero: bool) -> Option<()>
where
    T: Fn(Uint256, Uint256) -> Uint256,
    U: Fn(BigUint, BigUint) -> BigUint,
{
    let (lib, native) = generate_ints(data)?;
    let (lib2, native2) = loop {
        let (lib2, native2) = generate_ints(data)?;
        if ok_by_zero || !native2.is_zero() {
            break (lib2, native2);
        }
    };
    assert_same!(op_lib(lib, lib2), op_native(native, native2), "lib: {lib}, lib2: {lib2}");
    Some(())
}

fuzz_target!(|data: &[u8]| {
    let mut data = data;
    // from_le_bytes
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_same!(lib, native, "lib: {lib}");
    }
    let mask = &BigUint::from_bytes_le(&[u8::MAX; 32]);

    // Full addition
    assert_op(&mut data, Add::add, |a, b| (a + b) & mask, true);
    // Full multiplication
    assert_op(&mut data, Mul::mul, |a, b| (a * b) & mask, true);
    // Full division
    assert_op(&mut data, Div::div, |a, b| (a / b) & mask, false);
    // Full remainder
    assert_op(&mut data, Rem::rem, |a, b| (a % b) & mask, false);
    // Full bitwise And
    assert_op(&mut data, BitAnd::bitand, BitAnd::bitand, true);
    // Full bitwise Or
    assert_op(&mut data, BitOr::bitor, BitOr::bitor, true);
    // Full bitwise Xor
    assert_op(&mut data, BitXor::bitxor, BitXor::bitxor, true);

    // u64 addition
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let word = u64::from_le_bytes(try_opt!(consume(&mut data)));
        assert_same!(lib + word, (native + word) & mask, "lib: {lib}, word: {word}");
    }
    // U64 multiplication
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let word = u64::from_le_bytes(try_opt!(consume(&mut data)));
        assert_same!(lib * word, (native * word) & mask, "lib: {lib}, word: {word}");
    }
    // Left shift
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let lshift = try_opt!(consume::<1>(&mut data))[0] as u32;
        assert_same!(lib << lshift, (native << lshift) & mask, "lib: {lib}, lshift: {lshift}");
    }
    // Right shift
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let rshift = try_opt!(consume::<1>(&mut data))[0] as u32;
        assert_same!(lib >> rshift, (native >> rshift) & mask, "lib: {lib}, rshift: {rshift}");
    }
    // bits
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(u64::from(lib.bits()), native.bits(), "native: {native}");
    }
    // as u64
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let native_u64 = native.iter_u64_digits().next().unwrap_or_default();
        assert_eq!(lib.as_u64(), native_u64, "lib: {lib}");
    }
    // as u128
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let mut iter = native.iter_u64_digits();
        let new_native = (iter.next().unwrap_or_default() as u128) | ((iter.next().unwrap_or_default() as u128) << 64);
        assert_eq!(lib.as_u128(), new_native, "native: {native}");
    }
    // to_le_bytes
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_same!(lib, native, "lib: {lib}");
    }

    // to_be_bytes
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_same!(lib, native, "lib: {lib}");
    }

    // iter_be_bits
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        for (i, lib_bit) in lib.iter_be_bits().enumerate() {
            if let Some(native_bit_location) = (256 - i).checked_sub(1) {
                assert_eq!(
                    lib_bit,
                    native.bit(native_bit_location as u64),
                    "lib: {lib}, i: {i}, native_bit_loc: {native_bit_location}"
                );
            } else {
                assert!(!lib_bit);
            }
        }
    }
});
