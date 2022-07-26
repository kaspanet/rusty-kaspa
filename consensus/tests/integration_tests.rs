extern crate consensus;

use consensus::model::api::hash::Hash;
use consensus::processes::reachability::interval;
use std::str::FromStr;

/// Placeholder for actual integration tests
#[test]
fn integration_test() {
    let interval = interval::Interval::maximal();
    println!("{:?}", interval);

    let hash_str = "8e40af02265360d59f4ecf9ae9ebf8f00a3118408f5a9cdcbcc9c0f93642f3af";
    let hash = Hash::from_str(hash_str).unwrap();
    println!("{:?}", hash);
}
