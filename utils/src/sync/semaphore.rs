use event_listener::Event;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

#[cfg(feature = "semaphore-trace")]
mod trace {
    use super::*;
    use log::debug;
    use once_cell::sync::Lazy;
    use std::sync::atomic::AtomicU64;
    use std::time::SystemTime;

    static SYS_START: Lazy<SystemTime> = Lazy::new(SystemTime::now);

    #[inline]
    pub(super) fn sys_now() -> u64 {
        SystemTime::now().duration_since(*SYS_START).unwrap_or_default().as_micros() as u64
    }

    #[derive(Debug, Default)]
    pub struct TraceInner {
        readers_start: AtomicU64,
        readers_time: AtomicU64,
        log_time: AtomicU64,
        log_value: AtomicU64,
    }

    impl TraceInner {
        pub(super) fn mark_readers_start(&self) {
            self.readers_start.store(sys_now(), Ordering::Relaxed);
        }

        pub(super) fn mark_readers_end(&self) {
            let start = self.readers_start.load(Ordering::Relaxed);
            let now = sys_now();
            if start < now {
                let readers_time = self.readers_time.fetch_add(now - start, Ordering::Relaxed) + now - start;
                let log_time = self.log_time.load(Ordering::Relaxed);
                if log_time + (Duration::from_secs(10).as_micros() as u64) < now {
                    let log_value = self.log_value.load(Ordering::Relaxed);
                    debug!(
                        "Semaphore: log interval: {:?}, readers time: {:?}, fraction: {:.4}",
                        Duration::from_micros(now - log_time),
                        Duration::from_micros(readers_time - log_value),
                        (readers_time - log_value) as f64 / (now - log_time) as f64
                    );
                    self.log_value.store(readers_time, Ordering::Relaxed);
                    self.log_time.store(now, Ordering::Relaxed);
                }
            }
        }
    }
}

#[cfg(feature = "semaphore-trace")]
use trace::*;

#[cfg(feature = "semaphore-trace")]
pub(crate) fn get_module_path() -> &'static str {
    module_path!()
}

/// A low-level non-fair semaphore. The semaphore is non-fair in the sense that clients acquiring
/// a lower number of permits might get their allocation before earlier clients which requested more
/// permits -- if the semaphore can provide the lower allocation but not the larger. This non-fairness
/// is especially useful for implementing a strict readers-preferred reader-writer lock. See [`RfRwLock`].
/// Additionally it is possible that a new client immediately acquires if it happens to arrive right after
/// a release and before others were awaked. Otherwise the semaphore is usually fair in the sense that
/// waiters are awaked in the order they arrived at.
#[derive(Debug)]
pub(crate) struct Semaphore {
    counter: AtomicUsize,
    signal: Event,
    #[cfg(feature = "semaphore-trace")]
    trace_inner: TraceInner,
}

impl Semaphore {
    pub const MAX_PERMITS: usize = usize::MAX;

    pub fn new(available_permits: usize) -> Semaphore {
        cfg_if::cfg_if! {
            if #[cfg(feature = "semaphore-trace")] {
                Semaphore {
                    counter: AtomicUsize::new(available_permits),
                    signal: Event::new(),
                    trace_inner: Default::default(),
                }
            } else {
                Semaphore {
                    counter: AtomicUsize::new(available_permits),
                    signal: Event::new(),
                }
            }
        }
    }

    /// Tries to acquire `permits` slots from the semaphore. Upon success, returns the acquired slot
    pub fn try_acquire(&self, permits: usize) -> Option<usize> {
        let mut count = self.counter.load(Ordering::Acquire);
        loop {
            if count < permits {
                return None;
            }

            match self.counter.compare_exchange_weak(count, count - permits, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    #[cfg(feature = "semaphore-trace")]
                    if permits == 1 && count == Self::MAX_PERMITS {
                        // permits == 1 indicates a reader, count == Self::MAX_PERMITS indicates it is the first reader
                        self.trace_inner.mark_readers_start();
                    }
                    return Some(count);
                }
                Err(c) => count = c,
            }
        }
    }

    /// Asynchronously waits for `permits` permits to be acquired. Returns the acquired slot
    pub async fn acquire(&self, permits: usize) -> usize {
        let mut listener = None;

        loop {
            if let Some(slot) = self.try_acquire(permits) {
                return slot;
            }

            match listener.take() {
                None => listener = Some(self.signal.listen()),
                Some(l) => l.await,
            }
        }
    }

    /// Synchronously waits for `permits` permits to be acquired. Returns the acquired slot
    pub fn blocking_acquire(&self, permits: usize) -> usize {
        let mut listener = None;

        loop {
            if let Some(slot) = self.try_acquire(permits) {
                return slot;
            }

            match listener.take() {
                None => listener = Some(self.signal.listen()),
                Some(l) => l.wait(),
            }
        }
    }

    /// Releases a number of `permits` previously acquired by a call to [`acquire`] or [`acquire_blocking`].
    /// Returns the released slot
    pub fn release(&self, permits: usize) -> usize {
        let slot = self.counter.fetch_add(permits, Ordering::AcqRel) + permits;

        #[cfg(feature = "semaphore-trace")]
        if permits == 1 && slot == Self::MAX_PERMITS {
            // permits == 1 indicates a reader, slot == Self::MAX_PERMITS indicates it is the last reader
            self.trace_inner.mark_readers_end();
        }
        self.signal.notify(permits);
        slot
    }

    /// Releases and recaptures `permits` permits. Makes sure that other pending listeners get a
    /// chance to capture the emptied slots before this thread does so. Returns the acquired slot.
    pub fn blocking_yield(&self, permits: usize) -> usize {
        self.release(permits);
        // We wait for a signal or for a short timeout before we reenter the acquire loop.
        // Waiting for a signal has the benefit that if others are in the listen queue and they
        // capture the lock for less than timeout, then this thread will awake asap once they are
        // done. On the other hand a timeout is a must for the case where there are no other listeners
        // which will awake us.
        // Avoiding the wait all together is harmful in the case there are listeners, since this thread
        // will most likely recapture the emptied slot before they wake up.
        //
        // Tests and benchmarks show that 30 microseconds are sufficient for allowing other threads to capture the lock
        // (Windows: ~10 micros, Linux: 30 micros, Macos: 30 micros always worked with 2 yields which is sufficient for our needs)
        self.signal.listen().wait_timeout(Duration::from_micros(30));
        self.blocking_acquire(permits)
    }
}
