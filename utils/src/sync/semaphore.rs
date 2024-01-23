use event_listener::Event;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

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
}

impl Semaphore {
    pub const MAX_PERMITS: usize = usize::MAX;

    pub const fn new(available_permits: usize) -> Semaphore {
        Semaphore { counter: AtomicUsize::new(available_permits), signal: Event::new() }
    }

    /// Tries to acquire `permits` slots from the semaphore. Upon success, returns the acquired slot
    pub fn try_acquire(&self, permits: usize) -> Option<usize> {
        let mut count = self.counter.load(Ordering::Acquire);
        loop {
            if count < permits {
                return None;
            }

            match self.counter.compare_exchange_weak(count, count - permits, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return Some(count),
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
