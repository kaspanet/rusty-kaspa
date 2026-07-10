//! Per-inverse operation-count breakdown for `lehmer::invert`, to find where the
//! work goes. Run with:
//!
//!     cargo run -q --release --example lehmer_stats --features lehmer-instrument
//!
//! Build-independent (the counts come from the algorithm, not codegen).

#[cfg(not(feature = "lehmer-instrument"))]
fn main() {
    eprintln!("re-run with --features lehmer-instrument");
}

#[cfg(feature = "lehmer-instrument")]
fn main() {
    use core::sync::atomic::Ordering::Relaxed;
    use kaspa_math::Uint3072;
    use kaspa_math::lehmer::{instrument as ins, invert};
    use rand_chacha::{
        ChaCha8Rng,
        rand_core::{RngCore, SeedableRng},
    };

    const PRIME: Uint3072 = {
        let mut max = Uint3072::MAX;
        max.0[0] -= 1103717 - 1;
        max
    };

    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20000);
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let mut buf = [0u8; Uint3072::BYTES];
    let mut out = [0u64; 48];

    let mut done = 0u64;
    while done < n {
        rng.fill_bytes(&mut buf);
        let v = Uint3072::from_le_bytes(buf) % PRIME;
        if v.is_zero() {
            continue;
        }
        assert!(invert::<Uint3072>(v.0, PRIME.0, &mut out));
        done += 1;
    }

    let inv = ins::INVERSES.load(Relaxed) as f64;
    let m = ins::MATRICES.load(Relaxed) as f64;
    let fb = ins::FALLBACK_STEPS.load(Relaxed) as f64;
    let div = ins::DIVIDES.load(Relaxed) as f64;
    let axy = ins::APPLY_XY_LIMBS.load(Relaxed) as f64;
    let at = ins::APPLY_T_LIMBS.load(Relaxed) as f64;
    let norm = ins::GCD_DIV_NORM.load(Relaxed) as f64;

    println!("inverses measured : {inv:.0}");
    println!("per inverse:");
    println!("  matrices (apply) : {:.1}", m / inv);
    println!("  fallback steps   : {:.2}", fb / inv);
    println!("  divides (gcd_div): {:.1}", div / inv);
    println!("  apply_xy limbs   : {:.0}  (avg len {:.1}/matrix)", axy / inv, axy / m);
    println!("  apply_t  limbs   : {:.0}  (avg len {:.1}/matrix)", at / inv, at / m);
    println!("  divides/matrix   : {:.1}", div / m);
    // Raw count + a high-precision rate: this is ~0.0019/inverse, which a `{:.1}`
    // format would misleadingly round to 0.0 (it is rare, NOT unreachable).
    println!("  gcd_div q>d1     : {norm:.0} total, {:.5}/inverse ({:.2e} of divides)", norm / inv, norm / div);
    println!("  bits/matrix      : {:.1}  (3072 / matrices)", 3072.0 / (m / inv));
}
