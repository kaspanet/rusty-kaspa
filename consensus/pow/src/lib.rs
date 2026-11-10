//! Aero proof of work: [RandomX](https://github.com/tevador/RandomX), a CPU-oriented memory-hard hash.
//!
//! For a header, the hashed input is `PRE_POW_HASH (32) || TIME (8, LE) || NONCE (8, LE)` and the PoW value is the
//! 32-byte RandomX output read as a little-endian 256-bit integer, which must be `<= target`.
//!
//! The RandomX key is the constant [`RANDOMX_KEY`], so the cache and dataset are derived once per process. (Monero
//! rotates its key every ~2048 blocks; a constant key keeps validation of old headers, IBD and pruning proofs
//! simple, at the cost of the periodic re-initialization that rotation forces on miners.)
//!
//! - **Validation** uses light mode by default: a shared 256 MiB cache and one VM per thread, ~13 ms per hash. Set
//!   `AERO_RANDOMX_FAST_VALIDATION=1` to validate in fast mode instead (~1.5 ms per hash, needs ~2.5 GiB RAM).
//! - **Mining** uses fast mode ([`Hasher::fast`]): a ~2 GiB dataset derived from the same cache, ~10-50x faster.
//!
//! Both modes produce identical hashes. RandomX is a C++ library, so none of this exists on `wasm32`.

#[cfg(feature = "wasm32-sdk")]
pub mod wasm;

use std::cmp::max;

use aero_consensus_core::{BlockLevel, hashing, header::Header};
use aero_hashes::Hash;
use aero_math::Uint256;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{FAST_VALIDATION_ENV, Hasher, RANDOMX_KEY, cache, dataset};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::cell::RefCell;
    use std::sync::{Arc, OnceLock};

    use aero_randomx::{Cache, Dataset, Vm};

    /// Constant RandomX key; changing it changes consensus.
    pub const RANDOMX_KEY: &[u8] = b"aero/pow/randomx/v1";

    /// The shared 256 MiB cache, initialized on first use (about a second). Every node holds this.
    pub fn cache() -> Arc<Cache> {
        static CACHE: OnceLock<Arc<Cache>> = OnceLock::new();
        CACHE.get_or_init(|| Arc::new(Cache::new(RANDOMX_KEY).expect("failed to initialize the RandomX cache"))).clone()
    }

    /// The shared ~2 GiB dataset for mining, built on first use from [`cache`] using all cores. Validators never
    /// call this.
    pub fn dataset() -> Arc<Dataset> {
        static DATASET: OnceLock<Arc<Dataset>> = OnceLock::new();
        DATASET
            .get_or_init(|| {
                let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
                Arc::new(Dataset::new(&cache(), threads).expect("failed to build the RandomX dataset"))
            })
            .clone()
    }

    /// A RandomX VM (2 MiB scratchpad). Not shareable between threads: create one per thread and reuse it.
    pub struct Hasher(Vm);

    impl Hasher {
        /// Light mode (validation): uses the shared 256 MiB cache.
        pub fn light() -> Self {
            Self(Vm::light(cache()).expect("failed to create a RandomX VM"))
        }

        /// Fast mode (mining): uses the shared ~2 GiB dataset ([`dataset`]).
        pub fn fast() -> Self {
            Self(Vm::fast(dataset()).expect("failed to create a RandomX VM"))
        }

        #[inline]
        pub(crate) fn hash(&mut self, input: &[u8]) -> [u8; 32] {
            self.0.hash(input)
        }
    }

    /// Environment variable that makes this process *validate* with RandomX fast mode (set to anything but empty or `0`).
    /// Fast mode needs ~2.5 GiB of RAM but validates headers ~9x cheaper than light mode, which is what you want on
    /// seed nodes and servers. Hashes are identical either way, so it is safe to mix modes across the network.
    pub const FAST_VALIDATION_ENV: &str = "AERO_RANDOMX_FAST_VALIDATION";

    fn flag_enabled(value: Option<&std::ffi::OsStr>) -> bool {
        value.is_some_and(|v| !v.is_empty() && v != "0")
    }

    fn fast_validation() -> bool {
        static FAST: OnceLock<bool> = OnceLock::new();
        *FAST.get_or_init(|| flag_enabled(std::env::var_os(FAST_VALIDATION_ENV).as_deref()))
    }

    thread_local! {
        static VALIDATION_HASHER: RefCell<Option<Hasher>> = const { RefCell::new(None) };
    }

    /// Runs `f` with this thread's validation hasher (light mode, or fast mode if [`FAST_VALIDATION_ENV`] is set),
    /// creating it on first use.
    pub(crate) fn with_validation_hasher<R>(f: impl FnOnce(&mut Hasher) -> R) -> R {
        VALIDATION_HASHER
            .with(|slot| f(slot.borrow_mut().get_or_insert_with(|| if fast_validation() { Hasher::fast() } else { Hasher::light() })))
    }

    #[cfg(test)]
    mod tests {
        use super::flag_enabled;
        use std::ffi::OsStr;

        #[test]
        fn test_fast_validation_flag_parsing() {
            assert!(!flag_enabled(None));
            assert!(!flag_enabled(Some(OsStr::new(""))));
            assert!(!flag_enabled(Some(OsStr::new("0"))));
            assert!(flag_enabled(Some(OsStr::new("1"))));
            assert!(flag_enabled(Some(OsStr::new("true"))));
        }
    }
}

/// Proof-of-work state for one header: the target and the nonce-less part of the hashed input.
pub struct State {
    pub(crate) target: Uint256,
    // PRE_POW_HASH || TIME; the nonce is appended per evaluation
    pub(crate) prefix: [u8; 40],
}

impl State {
    #[inline]
    pub fn new(header: &Header) -> Self {
        let target = Uint256::from_compact_target_bits(header.bits);
        // Zero out the time and nonce.
        let pre_pow_hash = hashing::header::hash_override_nonce_time(header, 0, 0);
        Self::from_parts(pre_pow_hash, header.timestamp, target)
    }

    pub fn from_parts(pre_pow_hash: Hash, timestamp: u64, target: Uint256) -> Self {
        let mut prefix = [0u8; 40];
        prefix[..32].copy_from_slice(&pre_pow_hash.as_bytes());
        prefix[32..].copy_from_slice(&timestamp.to_le_bytes());
        Self { target, prefix }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[inline]
    fn input(&self, nonce: u64) -> [u8; 48] {
        let mut input = [0u8; 48];
        input[..40].copy_from_slice(&self.prefix);
        input[40..].copy_from_slice(&nonce.to_le_bytes());
        input
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl State {
    /// PoW value for `nonce` using this thread's light-mode hasher (validation).
    #[inline]
    #[must_use]
    pub fn calculate_pow(&self, nonce: u64) -> Uint256 {
        let hash = native::with_validation_hasher(|hasher| hasher.hash(&self.input(nonce)));
        Uint256::from_le_bytes(hash)
    }

    /// PoW value for `nonce` using a caller-owned hasher, light or fast (mining).
    #[inline]
    #[must_use]
    pub fn calculate_pow_with(&self, hasher: &mut Hasher, nonce: u64) -> Uint256 {
        Uint256::from_le_bytes(hasher.hash(&self.input(nonce)))
    }

    #[inline]
    #[must_use]
    pub fn check_pow(&self, nonce: u64) -> (bool, Uint256) {
        let pow = self.calculate_pow(nonce);
        // The pow hash must be less or equal than the claimed target.
        (pow <= self.target, pow)
    }

    /// Like [`State::check_pow`] with a caller-owned (typically fast-mode) hasher.
    #[inline]
    #[must_use]
    pub fn check_pow_with(&self, hasher: &mut Hasher, nonce: u64) -> (bool, Uint256) {
        let pow = self.calculate_pow_with(hasher, nonce);
        (pow <= self.target, pow)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn calc_block_level(header: &Header, max_block_level: BlockLevel) -> BlockLevel {
    let (block_level, _) = calc_block_level_check_pow(header, max_block_level);
    block_level
}

#[cfg(not(target_arch = "wasm32"))]
pub fn calc_block_level_check_pow(header: &Header, max_block_level: BlockLevel) -> (BlockLevel, bool) {
    if header.parents_by_level.is_empty() {
        return (max_block_level, true); // Genesis has the max block level
    }

    let state = State::new(header);
    let (passed, pow) = state.check_pow(header.nonce);
    let block_level = calc_level_from_pow(pow, max_block_level);
    (block_level, passed)
}

pub fn calc_level_from_pow(pow: Uint256, max_block_level: BlockLevel) -> BlockLevel {
    let signed_block_level = max_block_level as i64 - pow.bits() as i64;
    max(signed_block_level, 0) as BlockLevel
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    fn state() -> State {
        State::from_parts(Hash::from_bytes([7; 32]), 1_700_000_000_000, Uint256::MAX)
    }

    #[test]
    fn test_input_layout() {
        let input = state().input(0x0102030405060708);
        assert_eq!(&input[..32], &[7u8; 32]);
        assert_eq!(&input[32..40], &1_700_000_000_000u64.to_le_bytes());
        assert_eq!(&input[40..], &[8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn test_pow_is_deterministic_and_nonce_sensitive() {
        let state = state();
        assert_eq!(state.calculate_pow(5), state.calculate_pow(5));
        assert_ne!(state.calculate_pow(5), state.calculate_pow(6));
        // With the maximum target every nonce passes.
        assert!(state.check_pow(5).0);
    }

    #[test]
    fn test_target_is_enforced() {
        let state = State::from_parts(Hash::from_bytes([7; 32]), 1, Uint256::from_u64(0));
        // A zero target essentially never passes: the hash would have to be exactly zero.
        assert!(!state.check_pow(5).0);
    }

    /// Needs the ~2 GiB dataset: `cargo test -p aero-pow --release -- --ignored --nocapture`
    #[test]
    #[ignore = "allocates ~2 GiB"]
    fn test_fast_and_light_modes_agree() {
        let state = state();
        let mut fast = Hasher::fast();
        for nonce in 0..8 {
            assert_eq!(state.calculate_pow(nonce), state.calculate_pow_with(&mut fast, nonce));
        }
    }
}
