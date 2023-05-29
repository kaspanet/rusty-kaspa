use event_listener::Event;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A low-level non-fair semaphore. The semaphore is non-fair in the sense that clients acquiring
/// a lower number of permits might get their allocation before earlier clients which requested more
/// permits, if the semaphore can provide the lower allocation but not the larger. This is especially
/// useful for implementing a readers-preferred reader-writer lock
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
}
