use std::{
    ops::Deref,
    sync::atomic::{AtomicI32, Ordering},
};
use thiserror::Error;

static ACQUIRED_FD: AtomicI32 = AtomicI32::new(0);
#[derive(Debug)]
pub struct FDGuard(i32);

impl FDGuard {
    pub fn acquired(&self) -> i32 {
        self.0
    }
}

impl Deref for FDGuard {
    type Target = i32;

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
    pub acquired: i32,
    pub limit: i32,
}

pub fn acquire_guard(value: i32) -> Result<FDGuard, Error> {
    loop {
        let acquired = ACQUIRED_FD.load(Ordering::SeqCst); // todo ordering??
        let limit = limit();
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

#[cfg(not(target_arch = "wasm32"))]
pub fn try_set_fd_limit(limit: u64) -> std::io::Result<u64> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "windows")] {
                rlimit::setmaxstdio(limit as u32).map(|v| v as u64)
        } else if #[cfg(unix)] {
            rlimit::increase_nofile_limit(limit)
        }
    }
}

pub fn limit() -> i32 {
    cfg_if::cfg_if! {
        if #[cfg(test)] {
            100
        }
        else if #[cfg(target_os = "windows")] {
            rlimit::getmaxstdio() as i32
        }
        else if #[cfg(unix)] {
            rlimit::getrlimit(rlimit::Resource::NOFILE).unwrap().0 as i32
        }
        else {
            512
        }
    }
}

pub fn remainder() -> i32 {
    limit() - ACQUIRED_FD.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_and_release_guards() {
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
