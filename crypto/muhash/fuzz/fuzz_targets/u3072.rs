#![no_main]
use libfuzzer_sys::fuzz_target;
use kaspa_muhash::u3072::{self, U3072};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed};

fuzz_target!(|data: &[u8]| {
    if data.len() < muhash::SERIALIZED_MUHASH_SIZE {
        return;
    }

    let prime_num = prime_num();
    let mut start_uint = U3072::one();
    let mut start_num = BigInt::one();
    for current in data.chunks_exact(muhash::SERIALIZED_MUHASH_SIZE) {
        let current_uint = U3072::from_le_bytes(current.try_into().unwrap());
        let mut current_num = BigInt::from_bytes_le(num_bigint::Sign::Plus, current);

        if current[0] & 1 == 1 {
            start_uint /= current_uint;
            current_num = inverse_num(&current_num, &prime_num);
            start_num *= current_num;
        } else {
            start_uint *= current_uint;
            start_num *= current_num;
        }
        start_num %= &prime_num;
    }
    assert_eq!(start_uint.to_le_bytes(), num_to_le(&start_num));
});

fn num_to_le(n: &BigInt) -> [u8; muhash::SERIALIZED_MUHASH_SIZE] {
    let mut res = [0u8; muhash::SERIALIZED_MUHASH_SIZE];
    for (i, word) in n.iter_u64_digits().enumerate() {
        let part = &mut res[i*size_of::<u64>()..(i+1)*size_of::<u64>()];
        part.copy_from_slice(&word.to_le_bytes());
    }
    res
}

fn prime_num() -> BigInt {
    let mut prime = BigInt::one();
    prime <<= 3072;
    prime -= u3072::PRIME_DIFF;
    prime
}

fn inverse_num(n: &BigInt, prime: &BigInt) -> BigInt {
    let e_gcd = n.extended_gcd(prime);
    assert!(e_gcd.gcd.is_one() || &e_gcd.gcd == prime);
    if e_gcd.x.is_negative() {
        e_gcd.x + prime
    } else {
        e_gcd.x
    }
}
