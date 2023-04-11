#![no_main]
mod utils;

use core::ops::{Add, BitAnd, BitOr, BitXor, Div, Mul, Rem};
use kaspa_math::construct_uint;
use libfuzzer_sys::fuzz_target;
use std::convert::TryInto;
use utils::{consume, try_opt};

construct_uint!(Uint128, 2);

// Consumes 16 bytes
fn generate_ints(data: &mut &[u8]) -> Option<(Uint128, u128)> {
    let buf = consume(data)?;
    Some((Uint128::from_le_bytes(buf), u128::from_le_bytes(buf)))
}

// Consumes 16 bytes
fn generate_ints_top_bit_cleared(data: &mut &[u8]) -> Option<(Uint128, u128)> {
    let mut buf = consume(data)?;
    buf[15] &= 0b01111111; // clear the top/sign bit.
    Some((Uint128::from_le_bytes(buf), u128::from_le_bytes(buf)))
}

fn assert_op<T, U>(data: &mut &[u8], op_lib: T, op_native: U, ok_by_zero: bool) -> Option<()>
where
    T: Fn(Uint128, Uint128) -> Uint128,
    U: Fn(u128, u128) -> u128,
{
    let (lib, native) = generate_ints(data)?;
    let (lib2, native2) = loop {
        let (lib2, native2) = generate_ints(data)?;
        if ok_by_zero || native2 != 0 {
            break (lib2, native2);
        }
    };
    assert_eq!(op_lib(lib, lib2), op_native(native, native2), "native: {native}, native2: {native2}");
    Some(())
}

fuzz_target!(|data: &[u8]| {
    let mut data = data;
    // from_le_bytes
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib, native);
    }

    // Full addition
    assert_op(&mut data, Add::add, Add::add, true);
    // Full multiplication
    assert_op(&mut data, Mul::mul, Mul::mul, true);
    // Full division
    assert_op(&mut data, Div::div, Div::div, false);
    // Full remainder
    assert_op(&mut data, Rem::rem, Rem::rem, false);
    // Full bitwise And
    assert_op(&mut data, BitAnd::bitand, BitAnd::bitand, true);
    // Full bitwise Or
    assert_op(&mut data, BitOr::bitor, BitOr::bitor, true);
    // Full bitwise Xor
    assert_op(&mut data, BitXor::bitxor, BitXor::bitxor, true);

    // Full bitwise Not
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(!lib, !native, "native: {native}");
    }

    // u64 addition
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let word = u64::from_le_bytes(try_opt!(consume(&mut data)));
        assert_eq!(lib + word, native + (word as u128), "native: {native}, word: {word}");
    }
    // U64 multiplication
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let word = u64::from_le_bytes(try_opt!(consume(&mut data)));
        assert_eq!(lib * word, native * (word as u128), "native: {native}, word: {word}");
    }
    // Left shift
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let lshift = try_opt!(consume::<1>(&mut data))[0] as u32;
        assert_eq!(lib << lshift, native << lshift, "native: {native}, lshift: {lshift}");
    }
    // Right shift
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        let rshift = try_opt!(consume::<1>(&mut data))[0] as u32;
        assert_eq!(lib >> rshift, native >> rshift, "native: {native}, rshift: {rshift}");
    }
    // bits
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib.bits(), 128 - native.leading_zeros(), "native: {native}");
    }
    // as u64
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib.as_u64(), native as u64, "native: {native}");
    }
    // as u128
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib.as_u128(), native as u128, "native: {native}");
    }
    // as f64
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib.as_f64(), native as f64, "native: {native}");
    }
    // to_le_bytes
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib.to_le_bytes(), native.to_le_bytes(), "native: {native}");
    }

    // to_be_bytes
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        assert_eq!(lib.to_be_bytes(), native.to_be_bytes(), "native: {native}");
    }

    // iter_be_bits
    {
        let (lib, native) = try_opt!(generate_ints(&mut data));
        for (i, lib_bit) in lib.iter_be_bits().enumerate() {
            let native_bit = (native >> (127 - i)) & 1 == 1;
            assert_eq!(lib_bit, native_bit, "native: {native}");
        }
    }
    // mod_inv
    {
        // the modular inverse of 1 in Z/1Z is weird, should it be 1 or 0?
        // Also, 0 never has a mod_inverse
        let ((lib1, native1), (lib2, native2)) = loop {
            let (lib1, native1) = try_opt!(generate_ints_top_bit_cleared(&mut data));
            let (lib2, native2) = try_opt!(generate_ints_top_bit_cleared(&mut data));
            if lib1 < lib2 && lib1 != 0u64 {
                break ((lib1, native1), (lib2, native2));
            }
        };
        let lib_inv = lib1.mod_inverse(lib2);
        let native_inv = naive_mod_inv(native1, native2);
        assert_eq!(lib_inv.is_some(), native_inv.is_some());
        if let Some(lib_inv) = lib_inv {
            assert_eq!(lib_inv, native_inv.unwrap(), "lib1: {lib1}, lib2: {lib2}");
        }
    }
});

fn naive_mod_inv(x: u128, p: u128) -> Option<u128> {
    let mut t = 0;
    let mut newt = 1;
    let p: i128 = p.try_into().unwrap();
    let mut r: i128 = p;
    let mut newr: i128 = x.try_into().unwrap();

    while newr != 0 {
        let quotient = r / newr;
        (t, newt) = (newt, t - quotient * newt);
        (r, newr) = (newr, r - quotient * newr);
    }
    if r > 1 {
        return None;
    } else if t < 0 {
        t += p;
    }
    Some(t.try_into().unwrap())
}
