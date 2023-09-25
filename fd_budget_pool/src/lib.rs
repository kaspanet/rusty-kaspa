use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};

static ACQUIRED_FD: AtomicU64 = AtomicU64::new(0);
pub struct FDGuard {
    pub acquired: u64,
}

impl Drop for FDGuard {
    fn drop(&mut self) {
        ACQUIRED_FD.fetch_sub(self.acquired, Ordering::SeqCst); // todo ordering??
    }
}

pub fn acquire_guard(value: u64) -> Result<FDGuard, Box<dyn Error>> {
    loop {
        let current = ACQUIRED_FD.load(Ordering::SeqCst); // todo ordering??
        if current + value > get_limit() {
            return Err("Exceeded upper bound".into()); // todo thiserror, warning
        }
        // todo ordering??
        match ACQUIRED_FD.compare_exchange(current, current + value, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return Ok(FDGuard { acquired: value }),
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
            panic!("unsupported os")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let guard = acquire_guard(30).unwrap();
        assert_eq!(guard.acquired, 30);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 30);

        let guard2 = acquire_guard(80);
        assert!(guard2.is_err());
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 30);

        drop(guard);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 0);

        let guard = acquire_guard(100).unwrap();
        assert_eq!(guard.acquired, 100);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 100);
        drop(guard);
        assert_eq!(ACQUIRED_FD.load(Ordering::Relaxed), 0);

        let guard = acquire_guard(101);
        assert!(guard.is_err());
    }
}
