use std::{
    ops::Deref,
    sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

static ACQUIRED_FD: AtomicU64 = AtomicU64::new(0);
#[derive(Debug)]
pub struct FDGuard(u64);

impl FDGuard {
    pub fn acquired(&self) -> u64 {
        self.0
    }
}

impl Deref for FDGuard {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for FDGuard {
    fn drop(&mut self) {
        ACQUIRED_FD.fetch_sub(self.0, Ordering::SeqCst); // todo ordering??
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Error)]
#[error("Exceeded upper bound, acquired: {acquired}, limit: {limit}")]
pub struct Error {
    pub acquired: u64,
    pub limit: u64,
}

pub fn acquire_guard(value: u64) -> Result<FDGuard, Error> {
    loop {
        let acquired = ACQUIRED_FD.load(Ordering::SeqCst); // todo ordering??
        let limit = get_limit();
        if acquired + value > limit {
            return Err(Error { acquired, limit });
        }
        // todo ordering??
        match ACQUIRED_FD.compare_exchange(acquired, acquired + value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return Ok(FDGuard(value)),
            Err(_) => continue, // The global counter was updated by another thread, retry
        }
    }
}

pub fn get_limit() -> u64 {
    cfg_if::cfg_if! {
        if #[cfg(test)] {
            100
        }
        else if #[cfg(target_os = "windows")] {
            rlimit::getmaxstdio() as u64
        }
        else if #[cfg(any(target_os = "macos", target_os = "linux"))] {
            rlimit::getrlimit(rlimit::Resource::NOFILE).unwrap().0 as u64
        }
        else {
            panic!("unsupported OS")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let guard = acquire_guard(30).unwrap();
        assert_eq!(guard.acquired(), 30);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 30);

        let err = acquire_guard(80).unwrap_err();
        assert_eq!(err, Error { acquired: 30, limit: 100 });
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 30);

        drop(guard);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 0);

        let guard = acquire_guard(100).unwrap();
        assert_eq!(guard.acquired(), 100);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 100);
        drop(guard);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 0);

        let err = acquire_guard(101).unwrap_err();
        assert_eq!(err, Error { acquired: 0, limit: 100 });
    }
}
