#![cfg_attr(coverage_nightly, feature(no_coverage))] //This is required to skip certain tests, regarding code coverage.

pub mod rwlock;
pub(crate) mod semaphore;
