use kaspa_database::prelude::DB;
use rocksdb::{DBRawIteratorWithThreadMode, DBWithThreadMode, MultiThreaded};

pub type RawIter<'a> = DBRawIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>;

pub struct ReacquiringRawIterator<'a> {
    db: &'a DB,
    iter: RawIter<'a>,
    steps_until_reacquire: usize,
    reacquire_interval: usize,
}

impl<'a> ReacquiringRawIterator<'a> {
    pub fn new(db: &'a DB, reacquire_interval: usize) -> Self {
        let reacquire_interval = reacquire_interval.max(1);

        Self { db, iter: db.raw_iterator(), steps_until_reacquire: reacquire_interval, reacquire_interval }
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
        self.consume_budget_at_current();
    }

    pub fn next(&mut self) {
        self.iter.next();
        self.consume_budget_at_current();
    }

    pub fn status(&self) -> Result<(), rocksdb::Error> {
        self.iter.status()
    }

    fn consume_budget_at_current(&mut self) {
        if self.steps_until_reacquire <= 1 {
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
        self.reset_budget();
    }

    fn reset_budget(&mut self) {
        self.steps_until_reacquire = self.reacquire_interval;
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
        let mut reacq = ReacquiringRawIterator::new(&db, 3);

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

        // interval = 1 forces reacquire attempt after every next/seek.
        // The important edge: the final next lands past the last key exactly when
        // the budget says "reacquire now". The wrapper must not replace/re-seek/reset
        // into some different iterator state.
        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::new(&db, 1);

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

        // This next moves both iterators past the end. Since reacquire_interval = 1,
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
        let mut reacq = ReacquiringRawIterator::new(&db, 1);

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
        let mut reacq = ReacquiringRawIterator::new(&db, 1);

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
    fn reacquiring_iterator_matches_raw_iterator_across_many_reacquire_intervals() {
        let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        for i in 0..100u8 {
            db.put([i * 2], [i + 100]).unwrap();
        }

        let mut raw = db.raw_iterator();
        let mut reacq = ReacquiringRawIterator::new(&db, 7);

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
        let mut reacq = ReacquiringRawIterator::new(&db, 3);

        raw.seek([0]);
        reacq.seek([0]);
        assert_eq!(current(&raw), current(&reacq));

        raw.next();
        reacq.next();
        assert_eq!(current(&raw), Some((vec![1], vec![101])));
        assert_eq!(current(&raw), current(&reacq));

        // With reacquire interval 3, the seek to 0 and next to 1 consumed two budget
        // steps, so the next move to 2 triggers reacquisition at key 2.
        db.delete([2]).unwrap();

        raw.next();
        reacq.next();

        assert_eq!(current(&raw), Some((vec![2], vec![102])));
        assert_eq!(current(&reacq), Some((vec![3], vec![103])));
    }
}
