//! Benchmarks the two malachite-backed operations this branch replaces, each against its
//! replacement, to confirm the swaps are not regressions:
//!
//! 1. `mod_inverse` (muhash u3072.rs): inverse of a 3072-bit value modulo the MuHash prime
//!    2^3072 - 1103717, `[u64; 48]` limbs in/out (conversions inside the measured loop, the const
//!    modulus precomputed). malachite (current) vs the in-repo Lehmer ext-gcd (`lehmer.rs`), the
//!    chosen replacement. The rejected third-party candidates (dashu, crypto-bigint, num-bigint,
//!    ruint) are commented out and their dev-deps removed (re-add the deps and uncomment the
//!    marked modules/arms below to reproduce the full sweep).
//! 2. `ceiling_log_base_2` on u64 (consensus sync locator, sync/mod.rs): malachite vs plain std.
//!
//! Per inverse = group time / N (=32). Run: `cargo bench -p kaspa-math --bench candidates`.

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, black_box, criterion_group, criterion_main};
use rand_chacha::{
    ChaCha8Rng,
    rand_core::{RngCore, SeedableRng},
};

use kaspa_math::Uint3072;

const N: usize = 32;
const LIMBS: usize = 48;

// Same value as muhash UINT_PRIME: 2^3072 - 1103717
const PRIME: Uint3072 = {
    let mut max = Uint3072::MAX;
    max.0[0] -= 1103717 - 1;
    max
};

// Only used by the commented-out byte-marshalling candidates (dashu / num-bigint).
// fn limbs_to_le_bytes(limbs: &[u64; LIMBS]) -> [u8; LIMBS * 8] {
//     let mut bytes = [0u8; LIMBS * 8];
//     for (chunk, limb) in bytes.chunks_exact_mut(8).zip(limbs) {
//         chunk.copy_from_slice(&limb.to_le_bytes());
//     }
//     bytes
// }
//
// fn le_bytes_to_limbs(bytes: &[u8]) -> [u64; LIMBS] {
//     let mut limbs = [0u64; LIMBS];
//     for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks(8)) {
//         let mut buf = [0u8; 8];
//         buf[..chunk.len()].copy_from_slice(chunk);
//         *limb = u64::from_le_bytes(buf);
//     }
//     limbs
// }

fn inputs() -> Vec<[u64; LIMBS]> {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut buf = [0u8; LIMBS * 8];
    (0..N)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            (Uint3072::from_le_bytes(buf) % PRIME).0
        })
        .collect()
}

mod current_malachite {
    use super::*;

    pub fn modinv(limbs: &[u64; LIMBS]) -> [u64; LIMBS] {
        Uint3072(*limbs).mod_inverse(PRIME).expect("0 < a < prime").0
    }
}

// Euclidean Lehmer ext-gcd (in-repo, fixed 48-limb), the chosen permissive replacement.
// Avoids safegcd's full-width modular cofactor updates: clean integer cofactors that grow
// gradually, one sign fixup at the end.
mod cand_lehmer {
    use super::*;

    pub fn modinv(limbs: &[u64; LIMBS]) -> [u64; LIMBS] {
        let mut out = [0u64; LIMBS];
        assert!(kaspa_math::lehmer::invert(*limbs, PRIME.0, &mut out), "0 < a < prime");
        out
    }
}

// Ruled-out third-party candidates, kept for reference (per-inverse, latest run: dashu ~23.4us,
// crypto-bigint vartime ~114us / const-time ~126us, vs lehmer ~13.6us and malachite ~15.7us;
// earlier runs: num-bigint ~513us, ruint ~72us). The `dashu-int` / `crypto-bigint` dev-deps were
// removed from Cargo.toml once Lehmer won, so these modules and their bench arms are commented out.
// Re-add the deps and uncomment to reproduce.
//
// mod cand_num_bigint {
//     use super::*;
//     use num_bigint::BigUint;
//
//     pub fn prime() -> BigUint {
//         BigUint::from_bytes_le(&limbs_to_le_bytes(&PRIME.0))
//     }
//
//     pub fn modinv(limbs: &[u64; LIMBS], prime: &BigUint) -> [u64; LIMBS] {
//         let a = BigUint::from_bytes_le(&limbs_to_le_bytes(limbs));
//         let inv = a.modinv(prime).expect("0 < a < prime");
//         le_bytes_to_limbs(&inv.to_bytes_le())
//     }
// }
//
// mod cand_crypto_bigint {
//     use super::*;
//     pub use crypto_bigint::{Odd, U3072};
//
//     pub fn prime() -> Odd<U3072> {
//         Odd::new(U3072::from_words(PRIME.0)).expect("prime is odd")
//     }
//
//     // `from_words`/`to_words` are const `[u64; 48]` copies (uint.rs:132/148), i.e. ~free; the
//     // measured cost here is the safegcd inversion itself.
//     pub fn modinv_ct(limbs: &[u64; LIMBS], prime: &Odd<U3072>) -> [u64; LIMBS] {
//         let a = U3072::from_words(*limbs);
//         let inv = Option::from(a.invert_odd_mod(prime)).expect("0 < a < prime");
//         let inv: U3072 = inv;
//         inv.to_words()
//     }
//
//     pub fn modinv_vartime(limbs: &[u64; LIMBS], prime: &Odd<U3072>) -> [u64; LIMBS] {
//         let a = U3072::from_words(*limbs);
//         let inv = Option::from(a.invert_odd_mod_vartime(prime)).expect("0 < a < prime");
//         let inv: U3072 = inv;
//         inv.to_words()
//     }
// }
//
// mod cand_ruint {
//     use super::*;
//     use ruint::Uint;
//
//     pub type U3072 = Uint<3072, LIMBS>;
//
//     pub fn prime() -> U3072 {
//         U3072::from_limbs(PRIME.0)
//     }
//
//     pub fn modinv(limbs: &[u64; LIMBS], prime: &U3072) -> [u64; LIMBS] {
//         let a = U3072::from_limbs(*limbs);
//         let inv = a.inv_mod(*prime).expect("0 < a < prime");
//         *inv.as_limbs()
//     }
// }
//
// mod cand_dashu {
//     use super::*;
//     use dashu_int::{IBig, UBig, ops::ExtendedGcd};
//
//     pub fn prime() -> UBig {
//         UBig::from_le_bytes(&limbs_to_le_bytes(&PRIME.0))
//     }
//
//     // dashu has no built-in modular inverse; derive it from the extended gcd
//     pub fn modinv(limbs: &[u64; LIMBS], prime: &UBig) -> [u64; LIMBS] {
//         let a = UBig::from_le_bytes(&limbs_to_le_bytes(limbs));
//         let (g, x, _) = a.gcd_ext(prime.clone());
//         assert_eq!(g, UBig::ONE);
//         let m = IBig::from(prime.clone());
//         let mut x = x % &m;
//         if x.sign() == dashu_int::Sign::Negative {
//             x += &m;
//         }
//         let inv = UBig::try_from(x).unwrap();
//         le_bytes_to_limbs(&inv.to_le_bytes())
//     }
// }

fn bench_candidate<F>(group: &mut BenchmarkGroup<WallTime>, name: &str, inputs: &[[u64; LIMBS]], f: F)
where
    F: Fn(&[u64; LIMBS]) -> [u64; LIMBS],
{
    group.bench_function(name, |b| {
        b.iter(|| {
            for limbs in inputs {
                black_box(f(black_box(limbs)));
            }
        });
    });
}

// mod_inverse: the malachite incumbent vs the in-repo Lehmer ext-gcd replacement.
fn bench_modinv_3072(c: &mut Criterion) {
    let inputs = inputs();

    // lehmer (the chosen replacement) must agree with the current implementation
    for limbs in &inputs {
        let expected = current_malachite::modinv(limbs);
        assert_eq!(cand_lehmer::modinv(limbs), expected);
        // ruled-out candidates (deps removed):
        // assert_eq!(cand_dashu::modinv(limbs, &da_prime), expected);
        // assert_eq!(cand_crypto_bigint::modinv_vartime(limbs, &cb_prime), expected);
        // assert_eq!(cand_crypto_bigint::modinv_ct(limbs, &cb_prime), expected);
    }

    let mut group = c.benchmark_group("modinv_3072_muhash_prime");
    bench_candidate(&mut group, "malachite (current)", &inputs, current_malachite::modinv);
    bench_candidate(&mut group, "lehmer (Euclidean ext-gcd)", &inputs, cand_lehmer::modinv);
    // ruled-out candidates (re-add the dev-deps + uncomment the modules above to reproduce):
    // let da_prime = cand_dashu::prime();
    // let cb_prime = cand_crypto_bigint::prime();
    // bench_candidate(&mut group, "dashu (gcd_ext)", &inputs, |l| cand_dashu::modinv(l, &da_prime));
    // bench_candidate(&mut group, "crypto-bigint 0.7 (safegcd vartime)", &inputs, |l| cand_crypto_bigint::modinv_vartime(l, &cb_prime));
    // bench_candidate(&mut group, "crypto-bigint 0.7 (safegcd const-time)", &inputs, |l| cand_crypto_bigint::modinv_ct(l, &cb_prime));
    group.finish();
}

// The other malachite usage (consensus/src/processes/sync/mod.rs) is CeilingLogBase2 on a u64,
// replaced on this branch with std; benched here to confirm std is no slower.
fn bench_ceil_log2(c: &mut Criterion) {
    use kaspa_math::uint::malachite_base::num::arithmetic::traits::CeilingLogBase2;

    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let values: Vec<u64> = (0..1024).map(|_| rng.next_u64() | 1).collect();

    for v in &values {
        assert_eq!(v.ceiling_log_base_2(), 64 - (v - 1).leading_zeros() as u64);
    }

    let mut group = c.benchmark_group("ceil_log2_u64");
    group.bench_function("malachite (current)", |b| {
        b.iter(|| {
            for &v in &values {
                black_box(black_box(v).ceiling_log_base_2());
            }
        });
    });
    group.bench_function("std", |b| {
        b.iter(|| {
            for &v in &values {
                black_box(64 - (black_box(v) - 1).leading_zeros() as u64);
            }
        });
    });
    group.finish();
}

criterion_group!(benches, bench_modinv_3072, bench_ceil_log2);
criterion_main!(benches);
