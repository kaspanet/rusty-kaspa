//! Benchmark for streaming SMT import (stack-based).
//!
//! Usage:
//!   cargo run --release --example bench_streaming_import -p kaspa-smt-store
//!   cargo run --release --example bench_streaming_import -p kaspa-smt-store -- --lanes=1000000

use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::time::Instant;
use std::{fs, iter};

use kaspa_database::create_temp_db;
use kaspa_database::prelude::ConnBuilder;
use kaspa_hashes::{Hash, ZERO_HASH};
use kaspa_smt_store::processor::SmtStores;
use kaspa_smt_store::streaming_import::{StreamingImportLane, streaming_import};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

fn main() {
    let args = std::env::args();
    let lane_count: u64 =
        args.into_iter().find_map(|arg| arg.split_once("--lanes=").and_then(|(_, r)| r.parse().ok())).unwrap_or(100_000);

    println!("Bench streaming SMT import: {lane_count} lanes");

    let data_path = data_file_path(lane_count);
    if !data_path.exists() {
        println!("Generating lane data to {} ...", data_path.display());
        generate_lane_data(&data_path, lane_count);
        println!("Generated {} MB", fs::metadata(&data_path).unwrap().len() / (1024 * 1024));
    } else {
        println!("Using cached lane data: {}", data_path.display());
    }

    let rss_before = current_rss_mb();
    let lanes = read_lane_data(&data_path);

    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(512));
    let stores = SmtStores::new(db.clone(), 1, 1);

    let batch_size = 8192;
    let t_import = Instant::now();
    let result = streaming_import(&db, &stores, 100, ZERO_HASH, lane_count, lanes, batch_size).unwrap();
    let import_ms = t_import.elapsed().as_millis();

    let rss_after = current_rss_mb();
    println!("  Import: {import_ms} ms");
    println!("  Root: {}", result.root);
    println!("  Lanes imported: {}", result.lanes_imported);
    println!("  Nodes written: {}", result.nodes_written);
    println!("  RSS: {rss_before} MB -> {rss_after} MB (delta: {} MB)", rss_after.saturating_sub(rss_before));
}

fn data_file_path(count: u64) -> PathBuf {
    PathBuf::from(format!("target/smt-bench-lanes-{count}.bin"))
}

fn generate_lane_data(path: &PathBuf, count: u64) {
    let mut rng = StdRng::seed_from_u64(42);
    let mut lanes: Vec<(Hash, Hash, u64)> = (0..count)
        .map(|_| {
            let mut k = [0u8; 32];
            let mut t = [0u8; 32];
            rng.fill(&mut k);
            rng.fill(&mut t);
            let bs = rng.gen_range(1..=1_000_000u64);
            (Hash::from_bytes(k), Hash::from_bytes(t), bs)
        })
        .collect();

    lanes.sort_by_key(|(k, _, _)| *k);
    lanes.dedup_by_key(|(k, _, _)| *k);

    let mut file = fs::File::create(path).unwrap();
    let actual_count = lanes.len() as u64;
    file.write_all(&actual_count.to_le_bytes()).unwrap();
    for (k, t, bs) in &lanes {
        file.write_all(k.as_slice()).unwrap();
        file.write_all(t.as_slice()).unwrap();
        file.write_all(&bs.to_le_bytes()).unwrap();
    }
    file.flush().unwrap();
    file.sync_all().unwrap();
}

fn read_lane_data(path: &PathBuf) -> impl Iterator<Item = StreamingImportLane> {
    let file = fs::File::open(path).unwrap();
    let mut reader = BufReader::with_capacity(256 * 1024, file);
    let mut buf8 = [0u8; 8];
    reader.read_exact(&mut buf8).unwrap();
    let count = u64::from_le_bytes(buf8) as usize;
    let mut i = 0usize;
    iter::from_fn(move || {
        if i < count {
            let mut kbuf = [0u8; 32];
            let mut tbuf = [0u8; 32];
            reader.read_exact(&mut kbuf).unwrap();
            reader.read_exact(&mut tbuf).unwrap();
            reader.read_exact(&mut buf8).unwrap();
            i += 1;
            Some(StreamingImportLane {
                lane_key: Hash::from_bytes(kbuf),
                lane_tip: Hash::from_bytes(tbuf),
                blue_score: u64::from_le_bytes(buf8),
            })
        } else {
            None
        }
    })
}

fn current_rss_mb() -> u64 {
    if let Ok(status) = fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let kb: u64 = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                return kb / 1024;
            }
        }
    }
    0
}
