#![no_main]
// Fuzzes the in-repo 3072-bit modular inverse (`kaspa_math::lehmer::invert`) that replaced the
// malachite-backed generic `mod_inverse`. This is the only production modular inverse (muhash
// finalize, mod the MuHash prime 2^3072 - 1103717). Each nonzero reduced value is inverted and
// cross-checked bit-for-bit against a num-bigint extended-gcd reference.
use kaspa_math::{lehmer, Uint3072};
use libfuzzer_sys::fuzz_target;
use num_bigint::{BigInt, Sign};
use num_integer::Integer;
use num_traits::{One, Signed};

const LIMBS: usize = 48;
const PRIME_DIFF: u64 = 1103717;

// MuHash prime: 2^3072 - 1103717
fn prime_uint() -> Uint3072 {
    let mut max = Uint3072::MAX;
    max.0[0] -= PRIME_DIFF - 1;
    max
}

fn prime_num() -> BigInt {
    let mut prime = BigInt::one();
    prime <<= 3072;
    prime -= PRIME_DIFF;
    prime
}

fuzz_target!(|data: &[u8]| {
    if data.len() < Uint3072::BYTES {
        return;
    }
    let prime = prime_uint();
    let buf: [u8; Uint3072::BYTES] = data[..Uint3072::BYTES].try_into().unwrap();
    let value = Uint3072::from_le_bytes(buf) % prime;
    if value.is_zero() {
        return; // 0 has no modular inverse
    }

    // The production inverse under test.
    let mut out = [0u64; LIMBS];
    let invertible = lehmer::invert(value.0, prime.0, &mut out);
    // The MuHash prime is prime, so every nonzero reduced value is invertible.
    assert!(invertible, "value={value}");

    // num-bigint extended-gcd reference, compared bit-for-bit.
    let value_num = BigInt::from_bytes_le(Sign::Plus, &value.to_le_bytes());
    let expected = inverse_num(&value_num, &prime_num());
    let got = BigInt::from_bytes_le(Sign::Plus, &Uint3072(out).to_le_bytes());
    assert_eq!(got, expected, "value={value}");
});

fn inverse_num(n: &BigInt, prime: &BigInt) -> BigInt {
    let e_gcd = n.extended_gcd(prime);
    assert!(e_gcd.gcd.is_one());
    if e_gcd.x.is_negative() {
        e_gcd.x + prime
    } else {
        e_gcd.x
    }
}
