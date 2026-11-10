//! Minimal safe wrapper over the vendored [RandomX](https://github.com/tevador/RandomX) library.
//!
//! RandomX has two modes sharing one hash function, so both produce identical output:
//! - **light** ([`Vm::light`]): needs only the 256 MiB [`Cache`]. This is what validators use.
//! - **fast** ([`Vm::fast`]): needs the ~2 GiB [`Dataset`], derived from the cache. This is what miners use.
//!
//! The hash depends only on the `key` the cache was initialized with and the input, never on CPU flags.

use std::ffi::c_void;
use std::fmt;
use std::os::raw::{c_int, c_ulong};
use std::ptr::NonNull;
use std::sync::Arc;

pub const HASH_SIZE: usize = 32;

mod ffi {
    use super::*;

    #[repr(C)]
    pub struct RandomxCache {
        _private: [u8; 0],
    }
    #[repr(C)]
    pub struct RandomxDataset {
        _private: [u8; 0],
    }
    #[repr(C)]
    pub struct RandomxVm {
        _private: [u8; 0],
    }

    pub const FLAG_FULL_MEM: c_int = 4;
    pub const FLAG_JIT: c_int = 8;

    unsafe extern "C" {
        pub fn randomx_get_flags() -> c_int;
        pub fn randomx_alloc_cache(flags: c_int) -> *mut RandomxCache;
        pub fn randomx_init_cache(cache: *mut RandomxCache, key: *const c_void, key_size: usize);
        pub fn randomx_release_cache(cache: *mut RandomxCache);
        pub fn randomx_alloc_dataset(flags: c_int) -> *mut RandomxDataset;
        pub fn randomx_dataset_item_count() -> c_ulong;
        pub fn randomx_init_dataset(dataset: *mut RandomxDataset, cache: *mut RandomxCache, start_item: c_ulong, item_count: c_ulong);
        pub fn randomx_release_dataset(dataset: *mut RandomxDataset);
        pub fn randomx_create_vm(flags: c_int, cache: *mut RandomxCache, dataset: *mut RandomxDataset) -> *mut RandomxVm;
        pub fn randomx_destroy_vm(vm: *mut RandomxVm);
        pub fn randomx_calculate_hash(vm: *mut RandomxVm, input: *const c_void, input_size: usize, output: *mut c_void);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The 256 MiB cache could not be allocated.
    CacheAllocation,
    /// The ~2 GiB dataset could not be allocated.
    DatasetAllocation,
    /// The virtual machine could not be created.
    VmCreation,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CacheAllocation => write!(f, "failed to allocate the 256 MiB RandomX cache"),
            Error::DatasetAllocation => write!(f, "failed to allocate the 2 GiB RandomX dataset"),
            Error::VmCreation => write!(f, "failed to create the RandomX virtual machine"),
        }
    }
}

impl std::error::Error for Error {}

/// Flags RandomX recommends for this CPU (JIT, hardware AES, SIMD Argon2). They only affect speed.
fn recommended_flags() -> c_int {
    // SAFETY: pure query of CPU features.
    unsafe { ffi::randomx_get_flags() }
}

/// The 256 MiB cache derived from the key. Read-only after construction, so it can be shared across threads.
pub struct Cache {
    ptr: NonNull<ffi::RandomxCache>,
    flags: c_int,
}

// SAFETY: the cache is never mutated after `randomx_init_cache`; concurrent VMs only read from it.
unsafe impl Send for Cache {}
unsafe impl Sync for Cache {}

impl Cache {
    /// Allocates and initializes the cache (roughly a second of work).
    pub fn new(key: &[u8]) -> Result<Self, Error> {
        let mut flags = recommended_flags();
        // SAFETY: plain allocation call; null is handled.
        let mut raw = unsafe { ffi::randomx_alloc_cache(flags) };
        if raw.is_null() && flags & ffi::FLAG_JIT != 0 {
            // JIT can be unavailable (e.g. W^X enforcement); the interpreter gives the same hashes, only slower.
            flags &= !ffi::FLAG_JIT;
            raw = unsafe { ffi::randomx_alloc_cache(flags) };
        }
        let ptr = NonNull::new(raw).ok_or(Error::CacheAllocation)?;
        // SAFETY: `ptr` is a live cache; `key` is valid for `key.len()` bytes.
        unsafe { ffi::randomx_init_cache(ptr.as_ptr(), key.as_ptr().cast(), key.len()) };
        Ok(Self { ptr, flags })
    }
}

impl Drop for Cache {
    fn drop(&mut self) {
        // SAFETY: allocated by `randomx_alloc_cache` and released exactly once.
        unsafe { ffi::randomx_release_cache(self.ptr.as_ptr()) }
    }
}

/// The ~2 GiB dataset used by fast mode. Read-only after construction.
pub struct Dataset {
    ptr: NonNull<ffi::RandomxDataset>,
    vm_flags: c_int,
}

// SAFETY: fully initialized in `Dataset::new` and only read afterwards.
unsafe impl Send for Dataset {}
unsafe impl Sync for Dataset {}

struct SendPtr<T>(*mut T);
// SAFETY: used only to hand disjoint item ranges of one dataset to scoped threads.
unsafe impl<T> Send for SendPtr<T> {}

impl Dataset {
    /// Builds the dataset from `cache` using `threads` threads (a minute-ish single-threaded, seconds on many cores).
    pub fn new(cache: &Cache, threads: usize) -> Result<Self, Error> {
        // SAFETY: plain allocation call; null is handled.
        let raw = unsafe { ffi::randomx_alloc_dataset(0) };
        let ptr = NonNull::new(raw).ok_or(Error::DatasetAllocation)?;

        let total = unsafe { ffi::randomx_dataset_item_count() } as usize;
        let threads = threads.clamp(1, total);
        let per_thread = total / threads;
        std::thread::scope(|scope| {
            for i in 0..threads {
                let start = i * per_thread;
                // The last thread also takes the remainder.
                let count = if i == threads - 1 { total - start } else { per_thread };
                let (dataset, cache_ptr) = (SendPtr(ptr.as_ptr()), SendPtr(cache.ptr.as_ptr()));
                scope.spawn(move || {
                    let (dataset, cache_ptr) = (dataset, cache_ptr);
                    // SAFETY: threads initialize disjoint item ranges; the cache is only read.
                    unsafe { ffi::randomx_init_dataset(dataset.0, cache_ptr.0, start as c_ulong, count as c_ulong) };
                });
            }
        });

        Ok(Self { ptr, vm_flags: cache.flags | ffi::FLAG_FULL_MEM })
    }
}

impl Drop for Dataset {
    fn drop(&mut self) {
        // SAFETY: allocated by `randomx_alloc_dataset` and released exactly once.
        unsafe { ffi::randomx_release_dataset(self.ptr.as_ptr()) }
    }
}

enum Backing {
    #[allow(dead_code)] // kept alive for the VM's raw pointer
    Light(Arc<Cache>),
    #[allow(dead_code)]
    Fast(Arc<Dataset>),
}

/// A RandomX virtual machine: owns a 2 MiB scratchpad, so create one per thread and reuse it.
pub struct Vm {
    ptr: NonNull<ffi::RandomxVm>,
    _backing: Backing,
}

// SAFETY: a VM is single-threaded state, but nothing ties it to the thread that created it.
unsafe impl Send for Vm {}

impl Vm {
    /// Light-mode VM (validation). Shares `cache` read-only.
    pub fn light(cache: Arc<Cache>) -> Result<Self, Error> {
        // SAFETY: `cache` outlives the VM through `_backing`.
        let raw = unsafe { ffi::randomx_create_vm(cache.flags, cache.ptr.as_ptr(), std::ptr::null_mut()) };
        let ptr = NonNull::new(raw).ok_or(Error::VmCreation)?;
        Ok(Self { ptr, _backing: Backing::Light(cache) })
    }

    /// Fast-mode VM (mining). Shares `dataset` read-only.
    pub fn fast(dataset: Arc<Dataset>) -> Result<Self, Error> {
        // SAFETY: `dataset` outlives the VM through `_backing`.
        let raw = unsafe { ffi::randomx_create_vm(dataset.vm_flags, std::ptr::null_mut(), dataset.ptr.as_ptr()) };
        let ptr = NonNull::new(raw).ok_or(Error::VmCreation)?;
        Ok(Self { ptr, _backing: Backing::Fast(dataset) })
    }

    pub fn hash(&mut self, input: &[u8]) -> [u8; HASH_SIZE] {
        let mut out = [0u8; HASH_SIZE];
        // SAFETY: `input` and `out` are valid for the given lengths; the VM is exclusively borrowed.
        unsafe { ffi::randomx_calculate_hash(self.ptr.as_ptr(), input.as_ptr().cast(), input.len(), out.as_mut_ptr().cast()) };
        out
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        // SAFETY: created by `randomx_create_vm` and destroyed exactly once, before its backing is dropped.
        unsafe { ffi::randomx_destroy_vm(self.ptr.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; HASH_SIZE]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// (key, input, expected) from RandomX's own test suite (`src/tests/tests.cpp`) and its API example.
    const VECTORS: &[(&[u8], &[u8], &str)] = &[
        (b"RandomX example key\0", b"RandomX example input\0", "8a48e5f9db45ab79d9080574c4d81954fe6ac63842214aff73c244b26330b7c9"),
        (b"test key 000", b"This is a test", "639183aae1bf4c9a35884cb46b09cad9175f04efd7684e7262a0ac1c2f0b4e3f"),
        (b"test key 000", b"Lorem ipsum dolor sit amet", "300a0adb47603dedb42228ccb2b211104f4da45af709cd7547cd049e9489c969"),
        (
            b"test key 001",
            b"sed do eiusmod tempor incididunt ut labore et dolore magna aliqua",
            "e9ff4503201c0c2cca26d285c93ae883f9b1d30c9eb240b820756f2d5a7905fc",
        ),
    ];

    #[test]
    fn test_light_mode_matches_official_vectors() {
        for (key, input, expected) in VECTORS {
            let cache = Arc::new(Cache::new(key).unwrap());
            let mut vm = Vm::light(cache).unwrap();
            assert_eq!(
                hex(vm.hash(input)),
                *expected,
                "key {:?} input {:?}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn test_vm_is_reusable_and_deterministic() {
        let cache = Arc::new(Cache::new(b"test key 000").unwrap());
        let mut vm = Vm::light(cache).unwrap();
        let a = vm.hash(b"This is a test");
        let b = vm.hash(b"Lorem ipsum dolor sit amet");
        assert_ne!(a, b);
        assert_eq!(a, vm.hash(b"This is a test"));
    }

    /// Builds the real ~2 GiB dataset: `cargo test -p aero-randomx --release -- --ignored --nocapture`
    #[test]
    #[ignore = "allocates ~2 GiB"]
    fn test_fast_mode_matches_light_mode() {
        let key = b"test key 000";
        let cache = Arc::new(Cache::new(key).unwrap());
        let start = std::time::Instant::now();
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
        let dataset = Arc::new(Dataset::new(&cache, threads).unwrap());
        println!("dataset built in {:?} with {threads} threads", start.elapsed());

        let mut fast = Vm::fast(dataset).unwrap();
        let mut light = Vm::light(cache).unwrap();
        for (k, input, expected) in VECTORS.iter().filter(|(k, ..)| *k == key) {
            let _ = k;
            assert_eq!(hex(fast.hash(input)), *expected);
            assert_eq!(fast.hash(input), light.hash(input));
        }

        const N: u32 = 2_000;
        let t = std::time::Instant::now();
        for i in 0..N {
            std::hint::black_box(fast.hash(&i.to_le_bytes()));
        }
        println!("fast : {:?} per hash (single thread)", t.elapsed() / N);
        const M: u32 = 300;
        let t = std::time::Instant::now();
        for i in 0..M {
            std::hint::black_box(light.hash(&i.to_le_bytes()));
        }
        println!("light: {:?} per hash (single thread)", t.elapsed() / M);
    }
}
