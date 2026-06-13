//! Evaluates replacing the malachite-* deps, based on all production big-uint usage:
//!
//! 1. `mod_inverse` (the only malachite-backed op, called from muhash u3072.rs): inverse of a
//!    3072-bit value modulo the MuHash prime 2^3072 - 1103717, with `[u64; 48]` limbs in/out.
//!    Limb conversions are inside the measured loop, modulus conversion is precomputed (it is
//!    a const at the call site).
//! 2. `ceiling_log_base_2` on u64 (consensus sync locator) vs plain std.
//! 3. The hand-rolled (repo-owned, license-clean) Uint ops actually used in production, each
//!    against the dashu equivalent, to decide whether wholesale migration makes sense or the
//!    macro should stay for everything but `mod_inverse`:
//!    - blue work accumulation, Uint192 add/sum (ghostdag protocol.rs)
//!    - difficulty window average, Uint320 sum / div u64 / mul u64 / min (difficulty.rs)
//!    - calc_work, Uint256 `(!t / (t + 1)) + 1` (difficulty.rs)
//!    - compact target bits codec round-trip (no candidate: repo-owned either way)

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, Criterion, black_box, criterion_group, criterion_main};
use dashu_int::UBig;
use rand_chacha::{
    ChaCha8Rng,
    rand_core::{RngCore, SeedableRng},
};

use kaspa_math::{Uint192, Uint256, Uint320, Uint3072};

const N: usize = 32;
const LIMBS: usize = 48;

// Same value as muhash UINT_PRIME: 2^3072 - 1103717
const PRIME: Uint3072 = {
    let mut max = Uint3072::MAX;
    max.0[0] -= 1103717 - 1;
    max
};

fn limbs_to_le_bytes(limbs: &[u64; LIMBS]) -> [u8; LIMBS * 8] {
    let mut bytes = [0u8; LIMBS * 8];
    for (chunk, limb) in bytes.chunks_exact_mut(8).zip(limbs) {
        chunk.copy_from_slice(&limb.to_le_bytes());
    }
    bytes
}

fn le_bytes_to_limbs(bytes: &[u8]) -> [u64; LIMBS] {
    let mut limbs = [0u64; LIMBS];
    for (limb, chunk) in limbs.iter_mut().zip(bytes.chunks(8)) {
        let mut buf = [0u8; 8];
        buf[..chunk.len()].copy_from_slice(chunk);
        *limb = u64::from_le_bytes(buf);
    }
    limbs
}

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

// Ruled out by earlier runs (per-inverse: num-bigint ~513us, ruint ~72us vs dashu ~22us and
// malachite ~16us); kept for reference. (crypto-bigint is no longer commented out: it is wired
// into both modinv_3072 groups below to settle the "safegcd must be fastest" question directly.)
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
mod cand_crypto_bigint {
    use super::*;
    pub use crypto_bigint::{Odd, U3072};

    pub fn prime() -> Odd<U3072> {
        Odd::new(U3072::from_words(PRIME.0)).expect("prime is odd")
    }

    // `from_words`/`to_words` are const `[u64; 48]` copies (uint.rs:132/148), i.e. ~free; the
    // measured cost here is the safegcd inversion itself. The exact-calc group below pre-builds
    // the `U3072` inputs to prove the conversion is not what makes crypto-bigint slow.
    pub fn modinv_ct(limbs: &[u64; LIMBS], prime: &Odd<U3072>) -> [u64; LIMBS] {
        let a = U3072::from_words(*limbs);
        let inv = Option::from(a.invert_odd_mod(prime)).expect("0 < a < prime");
        let inv: U3072 = inv;
        inv.to_words()
    }

    pub fn modinv_vartime(limbs: &[u64; LIMBS], prime: &Odd<U3072>) -> [u64; LIMBS] {
        let a = U3072::from_words(*limbs);
        let inv = Option::from(a.invert_odd_mod_vartime(prime)).expect("0 < a < prime");
        let inv: U3072 = inv;
        inv.to_words()
    }
}

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

// Euclidean Lehmer ext-gcd (in-repo, fixed 48-limb), the recommended permissive replacement.
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

mod cand_dashu {
    use super::*;
    use dashu_int::{IBig, UBig, ops::ExtendedGcd};

    pub fn prime() -> UBig {
        UBig::from_le_bytes(&limbs_to_le_bytes(&PRIME.0))
    }

    // dashu has no built-in modular inverse; derive it from the extended gcd
    pub fn modinv(limbs: &[u64; LIMBS], prime: &UBig) -> [u64; LIMBS] {
        let a = UBig::from_le_bytes(&limbs_to_le_bytes(limbs));
        let (g, x, _) = a.gcd_ext(prime.clone());
        assert_eq!(g, UBig::ONE);
        let m = IBig::from(prime.clone());
        let mut x = x % &m;
        if x.sign() == dashu_int::Sign::Negative {
            x += &m;
        }
        let inv = UBig::try_from(x).unwrap();
        le_bytes_to_limbs(&inv.to_le_bytes())
    }
}

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

fn bench_modinv_3072(c: &mut Criterion) {
    let inputs = inputs();

    let da_prime = cand_dashu::prime();
    let cb_prime = cand_crypto_bigint::prime();

    // all candidates must agree with the current implementation
    for limbs in &inputs {
        let expected = current_malachite::modinv(limbs);
        assert_eq!(cand_dashu::modinv(limbs, &da_prime), expected);
        assert_eq!(cand_lehmer::modinv(limbs), expected);
        assert_eq!(cand_crypto_bigint::modinv_vartime(limbs, &cb_prime), expected);
        assert_eq!(cand_crypto_bigint::modinv_ct(limbs, &cb_prime), expected);
    }

    let mut group = c.benchmark_group("modinv_3072_muhash_prime");
    bench_candidate(&mut group, "malachite (current)", &inputs, current_malachite::modinv);
    bench_candidate(&mut group, "dashu (gcd_ext)", &inputs, |l| cand_dashu::modinv(l, &da_prime));
    bench_candidate(&mut group, "lehmer (Euclidean ext-gcd)", &inputs, cand_lehmer::modinv);
    bench_candidate(&mut group, "crypto-bigint 0.7 (safegcd vartime)", &inputs, |l| cand_crypto_bigint::modinv_vartime(l, &cb_prime));
    bench_candidate(&mut group, "crypto-bigint 0.7 (safegcd const-time)", &inputs, |l| cand_crypto_bigint::modinv_ct(l, &cb_prime));
    group.finish();
}

// The exact-calculation comparison the malachite-vs-crypto-bigint question hinges on: every
// candidate's inputs are converted to its NATIVE type up front (outside the timed loop), so the
// loop times only the modular inversion, never the marshalling from our `[u64; 48]` structure.
//
// This isolates the algorithm. For crypto-bigint the conversion (`U3072::from_words`) is a free
// const limb copy anyway; for malachite the `Natural` (heap bignum) is pre-built, and even its
// `&Natural::mod_inverse(&Natural)` clones the operands internally (mod_inverse.rs:210) -- that
// allocation is intrinsic to malachite's algorithm, not our boundary, so it stays in the timing.
fn bench_modinv_3072_exact(c: &mut Criterion) {
    use kaspa_math::uint::malachite_base::num::arithmetic::traits::ModInverse;
    use kaspa_math::uint::malachite_nz::natural::Natural;

    let inputs = inputs();

    // malachite native: pre-built Naturals + modulus, no limb<->Natural marshalling in the loop.
    let mala_prime = Natural::from_limbs_asc(&PRIME.0);
    let mala_inputs: Vec<Natural> = inputs.iter().map(|l| Natural::from_limbs_asc(l)).collect();

    // crypto-bigint native: pre-built stack U3072 inputs + Odd<U3072> modulus.
    let cb_prime = cand_crypto_bigint::prime();
    let cb_inputs: Vec<cand_crypto_bigint::U3072> = inputs.iter().map(|l| cand_crypto_bigint::U3072::from_words(*l)).collect();

    // sanity: the two native paths must agree before we time them.
    for (m, cb) in mala_inputs.iter().zip(&cb_inputs) {
        let m_inv = m.mod_inverse(&mala_prime).expect("0 < a < prime");
        let cb_inv: cand_crypto_bigint::U3072 = Option::from(cb.invert_odd_mod_vartime(&cb_prime)).expect("0 < a < prime");
        assert_eq!(Natural::from_limbs_asc(&cb_inv.to_words()), m_inv);
    }

    let mut group = c.benchmark_group("modinv_3072_exact_calc");
    group.bench_function("malachite (current)", |b| {
        b.iter(|| {
            for v in black_box(&mala_inputs) {
                black_box(black_box(v).mod_inverse(black_box(&mala_prime)));
            }
        });
    });
    group.bench_function("crypto-bigint 0.7 (safegcd vartime)", |b| {
        b.iter(|| {
            for v in black_box(&cb_inputs) {
                black_box(black_box(v).invert_odd_mod_vartime(black_box(&cb_prime)));
            }
        });
    });
    group.bench_function("crypto-bigint 0.7 (safegcd const-time)", |b| {
        b.iter(|| {
            for v in black_box(&cb_inputs) {
                black_box(black_box(v).invert_odd_mod(black_box(&cb_prime)));
            }
        });
    });
    group.finish();
}

// The only other malachite usage (consensus/src/processes/sync/mod.rs) is CeilingLogBase2 on a
// u64; std covers it with no crate at all.
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

// Blue work accumulation as in ghostdag protocol.rs: summing per-block work values.
// Values are kept under 2^176 so 256 of them cannot overflow Uint192.
fn bench_blue_work_sum(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let works: Vec<Uint192> = (0..256).map(|_| Uint192([rng.next_u64(), rng.next_u64(), rng.next_u64() >> 16])).collect();
    let works_ubig: Vec<UBig> = works.iter().map(|w| UBig::from_words(&w.0)).collect();

    let expected: Uint192 = works.iter().copied().sum();
    assert_eq!(UBig::from_words(&expected.0), works_ubig.iter().fold(UBig::ZERO, |a, b| a + b));

    let mut group = c.benchmark_group("blue_work_sum_uint192");
    group.bench_function("kaspa-math (current)", |b| {
        b.iter(|| black_box(black_box(&works).iter().copied().sum::<Uint192>()));
    });
    group.bench_function("dashu", |b| {
        b.iter(|| black_box(black_box(&works_ubig).iter().fold(UBig::ZERO, |a, b| a + b)));
    });
    group.finish();
}

// Difficulty window average as in difficulty.rs:190-197: Uint320 sum of promoted Uint256
// targets, divide by window size, scale by duration ratio, clamp to max target.
fn bench_difficulty_window(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut buf = [0u8; 32];
    // realistic mainnet-scale targets (~2^192)
    let targets: Vec<Uint256> = (0..256)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            buf[24..].fill(0);
            Uint256::from_le_bytes(buf)
        })
        .collect();
    let targets_ubig: Vec<UBig> = targets.iter().map(|t| UBig::from_words(&t.0)).collect();

    let len = targets.len() as u64;
    let measured_duration: u64 = 263_000;
    let expected_duration: u64 = 256_000;
    let max_target = Uint320::from(Uint256::from_u64(1).wrapping_shl(255) - Uint256::from_u64(1));
    let max_target_ubig = UBig::from_words(&max_target.0);

    let current = |targets: &[Uint256]| -> Uint320 {
        let sum: Uint320 = targets.iter().map(|t| Uint320::from(*t)).sum();
        (sum / len * measured_duration / expected_duration).min(max_target)
    };
    let dashu = |targets: &[UBig]| -> UBig {
        let sum = targets.iter().fold(UBig::ZERO, |a, b| a + b);
        (sum / len * measured_duration / expected_duration).min(max_target_ubig.clone())
    };

    assert_eq!(UBig::from_words(&current(&targets).0), dashu(&targets_ubig));

    let mut group = c.benchmark_group("difficulty_window_uint320");
    group.bench_function("kaspa-math (current)", |b| {
        b.iter(|| black_box(current(black_box(&targets))));
    });
    group.bench_function("dashu", |b| {
        b.iter(|| black_box(dashu(black_box(&targets_ubig))));
    });
    group.finish();
}

// calc_work as in difficulty.rs:211-221: work = (!target / (target + 1)) + 1, narrowed to Uint192.
fn bench_calc_work(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut buf = [0u8; 32];
    let targets: Vec<Uint256> = (0..256)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            buf[24..].fill(0);
            Uint256::from_le_bytes(buf)
        })
        .collect();
    let targets_ubig: Vec<UBig> = targets.iter().map(|t| UBig::from_words(&t.0)).collect();
    let max_ubig = UBig::from_words(&Uint256::MAX.0);

    let current = |t: Uint256| -> Uint192 { ((!t / (t + 1u64)) + 1u64).try_into().expect("work < 2^192") };
    let dashu = |t: &UBig| -> UBig { (&max_ubig - t) / (t + UBig::ONE) + UBig::ONE };

    for (t, tu) in targets.iter().zip(&targets_ubig) {
        assert_eq!(UBig::from_words(&current(*t).0), dashu(tu));
    }

    let mut group = c.benchmark_group("calc_work_uint256");
    group.bench_function("kaspa-math (current)", |b| {
        b.iter(|| {
            for &t in black_box(&targets) {
                black_box(current(black_box(t)));
            }
        });
    });
    group.bench_function("dashu", |b| {
        b.iter(|| {
            for t in black_box(&targets_ubig) {
                black_box(dashu(black_box(t)));
            }
        });
    });
    group.finish();
}

// Compact target bits codec (math/src/lib.rs:64-95). Repo-owned code with no malachite
// involvement and no candidate equivalent; benched as a regression baseline only.
fn bench_compact_bits(c: &mut Criterion) {
    let mut rng = ChaCha8Rng::from_seed([42u8; 32]);
    let mut buf = [0u8; 32];
    let bits: Vec<u32> = (0..256)
        .map(|_| {
            rng.fill_bytes(&mut buf);
            buf[24..].fill(0);
            Uint256::from_le_bytes(buf).compact_target_bits()
        })
        .collect();

    let mut group = c.benchmark_group("compact_bits_uint256");
    group.bench_function("decode+encode (current)", |b| {
        b.iter(|| {
            for &v in black_box(&bits) {
                black_box(Uint256::from_compact_target_bits(black_box(v)).compact_target_bits());
            }
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_modinv_3072,
    bench_modinv_3072_exact,
    bench_ceil_log2,
    bench_blue_work_sum,
    bench_difficulty_window,
    bench_calc_work,
    bench_compact_bits
);
criterion_main!(benches);
