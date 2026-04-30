use kaspa_database::prelude::DB;
use rocksdb::{DBRawIteratorWithThreadMode, DBWithThreadMode, MultiThreaded};
use std::time::{Duration, Instant};

pub type RawIter<'a> = DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>;

const DEFAULT_REACQUIRE_STEPS: usize = 16384;
const DEFAULT_REACQUIRE_TIMEOUT: Duration = Duration::from_secs(1);

/// A RocksDB raw iterator wrapper that periodically recreates the underlying iterator.
///
/// RocksDB iterators can hold/pin DB resources while scanning. This wrapper keeps the
/// same cursor-shaped API for the operations used by this crate, but after enough
/// iterator actions or elapsed time it opens a fresh raw iterator and seeks it back to
/// the current key.
pub struct ReacquiringRawIterator<'a> {
    db: &'a DB,
    iter: RawIter<'a>,
    steps_until_reacquire: usize,
    reacquire_steps: usize,
    last_reacquire: Instant,
    reacquire_timeout: Duration,
}

impl<'a> ReacquiringRawIterator<'a> {
    /// Creates a reacquiring iterator with the default policy: capture DB resources
    /// for at most 1 second and at most 16,384 iterator actions between reacquisitions.
    pub fn new(db: &'a DB) -> Self {
        Self::with_policy(db, DEFAULT_REACQUIRE_STEPS, DEFAULT_REACQUIRE_TIMEOUT)
    }

    pub fn with_policy(db: &'a DB, reacquire_steps: usize, reacquire_timeout: Duration) -> Self {
        let reacquire_steps = reacquire_steps.max(1);

        Self {
            db,
            iter: db.raw_iterator(),
            steps_until_reacquire: reacquire_steps,
            reacquire_steps,
            last_reacquire: Instant::now(),
            reacquire_timeout,
        }
    }

    pub fn valid(&self) -> bool {
        self.iter.valid()
    }

    pub fn key(&self) -> Option<&[u8]> {
        self.iter.key()
    }

    pub fn value(&self) -> Option<&[u8]> {
        self.iter.value()
    }

    pub fn seek(&mut self, key: impl AsRef<[u8]>) {
        self.iter.seek(key.as_ref());
        self.reacquire_if_expired();
    }

    pub fn next(&mut self) {
        self.iter.next();
        self.reacquire_if_expired();
    }

    pub fn status(&self) -> Result<(), rocksdb::Error> {
        self.iter.status()
    }

    fn reacquire_if_expired(&mut self) {
        if self.steps_until_reacquire <= 1
            || Instant::now().checked_duration_since(self.last_reacquire).unwrap_or_default() >= self.reacquire_timeout
        {
            self.reacquire_at_current();
        } else {
            self.steps_until_reacquire -= 1;
        }
    }

    fn reacquire_at_current(&mut self) {
        if !self.iter.valid() {
            // Once invalid, the scan is considered complete; the caller is expected
            // to drop the cursor, preserving any iterator status for inspection.
            return;
        }

        let key = self.iter.key().expect("valid RocksDB iterator should have a key");

        let mut iter = self.db.raw_iterator();
        // `key` borrows from the old iterator, which is still alive here.
        // The old iterator is replaced only after this seek completes.
        iter.seek(key);

        self.iter = iter;
        self.reset_expiration();
    }

    fn reset_expiration(&mut self) {
        self.steps_until_reacquire = self.reacquire_steps;
        self.last_reacquire = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    trait TestCursor {
        fn valid(&self) -> bool;
        fn key(&self) -> Option<&[u8]>;
        fn value(&self) -> Option<&[u8]>;
    }

    impl TestCursor for RawIter<'_> {
        fn valid(&self) -> bool {
            self.valid()
        }

        fn key(&self) -> Option<&[u8]> {
            self.key()
        }

        fn value(&self) -> Option<&[u8]> {
            self.value()
        }
    }

    impl TestCursor for ReacquiringRawIterator<'_> {
        fn valid(&self) -> bool {
            self.valid()
        }

        fn key(&self) -> Option<&[u8]> {
            self.key()
        }

        fn value(&self) -> Option<&[u8]> {
            self.value()
        }
    }

    fn current(cursor: &impl TestCursor) -> Option<(Vec<u8>, Vec<u8>)> {
        if !cursor.valid() {
            return None;
        }
        Some((cursor.key()?.to_vec(), cursor.value()?.to_vec()))
    }

    #[test]
    fn reacquiring_iterator_matches_raw_iterator_for_seek_next_pattern() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..12u8 {
            db.put([i * 2], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 3, Duration::MAX);

        raw.seek([0]);
        reacq.seek([0]);
        assert_eq!(current(&raw), current(&reacq));

        for _ in 0..5 {
            raw.next();
            reacq.next();
            assert_eq!(current(&raw), current(&reacq));
        }

        raw.seek([7]);
        reacq.seek([7]);
        assert_eq!(current(&raw), current(&reacq));

        for _ in 0..3 {
            raw.next();
            reacq.next();
            assert_eq!(current(&raw), current(&reacq));
        }

        raw.seek([13]);
        reacq.seek([13]);
        assert_eq!(current(&raw), current(&reacq));

        for _ in 0..10 {
            raw.next();
            reacq.next();
            assert_eq!(current(&raw), current(&reacq));
        }

        raw.seek([23]);
        reacq.seek([23]);
        assert_eq!(current(&raw), current(&reacq));
    }

    #[test]
    fn reacquiring_iterator_preserves_invalid_state_on_boundary_reacquire() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..3u8 {
            db.put([i], [i + 100]).unwrap();
        }

        // reacquire_steps = 1 forces reacquire attempt after every next/seek.
        // The important edge: the final next lands past the last key exactly when
        // the budget says "reacquire now". The wrapper must not replace/re-seek/reset
        // into some different iterator state.
        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 1, Duration::MAX);

        raw.seek([0]);
        reacq.seek([0]);
        assert_eq!(current(&raw), current(&reacq));
        assert!(reacq.valid());

        raw.next();
        reacq.next();
        assert_eq!(current(&raw), current(&reacq));
        assert_eq!(current(&reacq), Some((vec![1], vec![101])));

        raw.next();
        reacq.next();
        assert_eq!(current(&raw), current(&reacq));
        assert_eq!(current(&reacq), Some((vec![2], vec![102])));

        // This next moves both iterators past the end. Since reacquire_steps = 1,
        // the wrapper attempts to reacquire exactly here, while invalid.
        raw.next();
        reacq.next();

        assert!(!raw.valid());
        assert!(!reacq.valid());
        assert_eq!(current(&raw), None);
        assert_eq!(current(&reacq), None);

        // Preserve the invalid/exhausted iterator state, including status.
        assert_eq!(raw.status().is_ok(), reacq.status().is_ok());

        // Repeated next after exhaustion should remain equivalent.
        raw.next();
        reacq.next();
        assert_eq!(current(&raw), current(&reacq));
        assert_eq!(raw.status().is_ok(), reacq.status().is_ok());
    }

    #[test]
    fn reacquiring_iterator_preserves_seek_to_missing_key_semantics_on_boundary() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..6u8 {
            db.put([i * 2], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 1, Duration::MAX);

        // 7 is missing; RocksDB should land on 8.
        raw.seek([7]);
        reacq.seek([7]);

        assert_eq!(current(&raw), current(&reacq));
        assert_eq!(current(&reacq), Some((vec![8], vec![104])));
        assert!(reacq.status().is_ok());
    }

    #[test]
    fn reacquiring_iterator_preserves_invalid_state_when_seek_lands_past_end_on_boundary() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..3u8 {
            db.put([i], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 1, Duration::MAX);

        raw.seek([99]);
        reacq.seek([99]);

        assert!(!raw.valid());
        assert!(!reacq.valid());
        assert_eq!(current(&raw), current(&reacq));
        assert_eq!(raw.status().is_ok(), reacq.status().is_ok());

        raw.next();
        reacq.next();

        assert_eq!(current(&raw), current(&reacq));
        assert_eq!(raw.status().is_ok(), reacq.status().is_ok());
    }

    #[test]
    fn reacquiring_iterator_matches_raw_iterator_across_many_reacquire_steps() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..100u8 {
            db.put([i * 2], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 7, Duration::MAX);

        for start in [0u8, 1, 17, 42, 99, 155, 199] {
            raw.seek([start]);
            reacq.seek([start]);
            assert_eq!(current(&raw), current(&reacq));

            for _ in 0..13 {
                raw.next();
                reacq.next();
                assert_eq!(current(&raw), current(&reacq));
                assert_eq!(raw.status().is_ok(), reacq.status().is_ok());
            }
        }
    }

    #[test]
    fn reacquiring_iterator_seeks_next_key_when_current_key_is_deleted_before_reacquire() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..4u8 {
            db.put([i], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 3, Duration::MAX);

        raw.seek([0]);
        reacq.seek([0]);
        assert_eq!(current(&raw), current(&reacq));

        raw.next();
        reacq.next();
        assert_eq!(current(&raw), Some((vec![1], vec![101])));
        assert_eq!(current(&raw), current(&reacq));

        // With reacquire_steps = 3, the seek to 0 and next to 1 consumed two
        // steps, so the next move to 2 triggers reacquisition at key 2.
        db.delete([2]).unwrap();

        raw.next();
        reacq.next();

        assert_eq!(current(&raw), Some((vec![2], vec![102])));
        assert_eq!(current(&reacq), Some((vec![3], vec![103])));
    }

    #[test]
    fn reacquiring_iterator_reacquires_when_timeout_elapses() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..4u8 {
            db.put([i], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::with_policy(&db, 100, Duration::ZERO);

        raw.seek([0]);
        reacq.seek([0]);
        assert_eq!(current(&raw), current(&reacq));

        raw.next();
        reacq.next();
        assert_eq!(current(&raw), Some((vec![1], vec![101])));
        assert_eq!(current(&raw), current(&reacq));

        // The step budget is far from exhausted, so this reacquisition is forced
        // only by the elapsed-time policy.
        db.delete([2]).unwrap();

        raw.next();
        reacq.next();

        assert_eq!(current(&raw), Some((vec![2], vec![102])));
        assert_eq!(current(&reacq), Some((vec![3], vec![103])));
    }
}
