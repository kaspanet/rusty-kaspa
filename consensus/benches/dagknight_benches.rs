use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::str::FromStr;
use std::sync::Arc;

use criterion::{Criterion, SamplingMode, black_box, criterion_group, criterion_main};
use kaspa_consensus_core::{KType, blockhash::ORIGIN, header::Header};
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use parking_lot::RwLock;

use kaspa_consensus::{
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            dagknight::MemoryDagknightStore, headers::MemoryHeaderStore, reachability::MemoryReachabilityStore,
            relations::MemoryRelationsStore,
        },
    },
    processes::{
        dagknight::protocol::DagknightExecutor,
        reachability::tests::{DagBlock, DagBuilder},
    },
};

/// Helper: converts a short hex ID (e.g. "70cd51") to a full Kaspa Hash.
fn prefixed_hash(s: &str) -> Hash {
    let mut hex = [b'0'; 64];
    hex[..s.len()].copy_from_slice(s.as_bytes());
    Hash::from_str(std::str::from_utf8(&hex).unwrap()).expect("Invalid hash string")
}

/// Builds the DAG from a JSON fixture file and runs `DagknightExecutor.dagknight` on the tips.
///
/// Returns the selected parent hash (the tie-breaking winner) and the number of
/// conflict-ordered parents.
#[allow(clippy::arc_with_non_send_sync)]
fn run_tie_breaking_at_k(k: KType) -> (Hash, usize) {
    let json_filename = format!("{}/tie_breaking_k{}.json", env!("CARGO_MANIFEST_DIR"), k);
    let file = File::open(&json_filename).expect("Unable to open JSON file");
    let json_data: serde_json::Value = serde_json::from_reader(file).expect("Unable to parse JSON");

    let tips: Vec<Hash> =
        json_data["tips"].as_array().expect("tips is not an array").iter().map(|t| prefixed_hash(t.as_str().expect("tip"))).collect();

    let blocks = json_data["blocks"].as_array().expect("blocks is not an array");

    let test_blocks: Vec<(Hash, Vec<Hash>, Uint192, u32, u64, u64, Hash)> = blocks
        .iter()
        .map(|block| {
            let id = prefixed_hash(block["id"].as_str().expect("id"));
            let parents: Vec<Hash> = if block["parents"].as_array().map(|a| a.is_empty()).unwrap_or(false) {
                vec![ORIGIN]
            } else {
                block["parents"].as_array().unwrap().iter().map(|p| prefixed_hash(p.as_str().expect("parent"))).collect()
            };
            let blue_work = Uint192::from_u64(block["blue_work"].as_str().expect("blue_work").parse::<u64>().unwrap());
            let bits = u32::from_str_radix(block["bits"].as_str().expect("bits"), 16).unwrap();
            let blue_score = block["blue_score"].as_u64().unwrap();
            let daa_score = block["daa_score"].as_u64().unwrap();
            let sp = if block["selected_parent"].is_null() {
                ORIGIN
            } else {
                prefixed_hash(block["selected_parent"].as_str().expect("selected_parent"))
            };
            let selected_parent = if parents.contains(&sp) { sp } else { ORIGIN };
            (id, parents, blue_work, bits, blue_score, daa_score, selected_parent)
        })
        .collect();

    // Setup stores
    let dk_map = RefCell::new(HashMap::new());
    let dagknight_store = Arc::new(MemoryDagknightStore::new(dk_map));
    let mut reachability = MemoryReachabilityStore::new();
    let mut relations = MemoryRelationsStore::new();
    let headers_store = Arc::new(MemoryHeaderStore::new());

    let dk_executor = DagknightExecutor {
        genesis_hash: ORIGIN,
        dagknight_store: dagknight_store.clone(),
        headers_store: headers_store.clone(),
        reachability_service: MTReachabilityService::new(Arc::new(RwLock::new(reachability.clone()))),
        relations_store: Arc::new(RwLock::new(relations.clone())),
    };

    // Build DAG: sort blocks by blue_work, insert into stores
    let mut builder = DagBuilder::new(&mut reachability, &mut relations);
    builder.init();

    let mut test_blocks = test_blocks;
    test_blocks.sort_by_key(|(_, _, blue_work, _, _, _, _)| *blue_work);

    for (hash, parents, blue_work, bits, blue_score, daa_score, selected_parent) in &test_blocks {
        let mut header = Header::from_precomputed_hash(*hash, parents.clone());
        header.bits = *bits;
        header.blue_work = *blue_work;
        header.blue_score = *blue_score;
        header.daa_score = *daa_score;
        headers_store.insert(Arc::new(header));

        builder.add_block_with_selected_parent(DagBlock::new(*hash, parents.clone()), *selected_parent);
    }

    // Run dagknight on tips — this triggers tie_breaking internally
    let dk_data = dk_executor.dagknight(&tips);

    (dk_data.selected_parent, dk_data.conflict_ordered_parents.len())
}

/// Criterion benchmark group for tie-breaking at various k values.
fn benchmark_tie_breaking(c: &mut Criterion) {
    let mut group = c.benchmark_group("dagknight/tie_breaking");
    group.sampling_mode(SamplingMode::Flat);

    for &k in &[4, 9, 16, 25, 36, 53] {
        group.bench_function(format!("k={}", k), |b| {
            b.iter(|| {
                let (selected_parent, conflict_count) = run_tie_breaking_at_k(black_box(k));
                black_box((selected_parent, conflict_count));
            })
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(std::time::Duration::new(30, 0));
    targets = benchmark_tie_breaking
}
criterion_main!(benches);
