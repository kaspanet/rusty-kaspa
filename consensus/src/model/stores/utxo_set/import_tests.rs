use super::*;
use kaspa_consensus_core::{muhash::MuHashExtensions, tx::ScriptPublicKey};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use kaspa_muhash::MuHash;
use rand::{Rng, SeedableRng, rngs::StdRng};
use rayon::prelude::*;
use std::{hint::black_box, time::Instant};

fn entries(count: usize, script_len: usize) -> Vec<(TransactionOutpoint, UtxoEntry)> {
    let mut rng = StdRng::seed_from_u64(1234);
    (0..count)
        .map(|i| {
            (
                TransactionOutpoint::new(Hash::from_bytes(rng.r#gen()), (i % 257) as u32),
                UtxoEntry {
                    amount: i as u64 + 1,
                    script_public_key: ScriptPublicKey::new(0, vec![0x51; script_len].as_slice().into()),
                    block_daa_score: 100,
                    is_coinbase: false,
                    covenant_id: None,
                },
            )
        })
        .collect()
}

fn rows(store: &DbUtxoSetStore) -> Vec<(TransactionOutpoint, Arc<UtxoEntry>)> {
    store.iterator().collect::<Result<_, _>>().unwrap()
}

#[test]
fn imported_batches_match_direct_writes_and_invalidate_copy_cache() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(32));
    for script_len in [34, 4096] {
        let mut legacy = DbUtxoSetStore::new(db.clone(), CachePolicy::Count(16), vec![80]);
        let mut active = DbUtxoSetStore::new(db.clone(), CachePolicy::Count(16), vec![81]);
        legacy.clear().unwrap();
        active.clear().unwrap();
        let mut values = entries(IMPORT_BATCH_ENTRIES + 7, script_len);
        // Last-write-wins must also hold across a batch boundary.
        let mut duplicate = values[0].clone();
        duplicate.1.amount += 10;
        values.push(duplicate);
        legacy.write_many(&values).unwrap();
        active.write_imported_chunk(&values).unwrap();
        active.write_imported_chunk(&[]).unwrap();
        assert_eq!(rows(&legacy), rows(&active));
        let uncached = active.clone_with_new_cache(CachePolicy::Empty);
        assert_eq!(rows(&legacy), rows(&uncached), "all tail writes must be persisted before return");

        let mut replacement = values[1].clone();
        replacement.1.amount += 20;
        assert_eq!(UtxoSetStoreReader::get(&active, &replacement.0).unwrap().amount, values[1].1.amount);
        active.copy_imported_from_iterator([(replacement.0, Arc::new(replacement.1.clone()))]).unwrap();
        assert_eq!(*UtxoSetStoreReader::get(&active, &replacement.0).unwrap(), replacement.1);
    }
}

#[test]
fn import_writer_flushes_at_entry_and_byte_bounds_and_keeps_tail() {
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(32));
    let mut writer = UtxoImportWriter::new(&db);
    for i in 0..IMPORT_BATCH_ENTRIES {
        writer.put((i as u64).to_le_bytes(), [1]).unwrap();
    }
    assert!(writer.batch.is_empty(), "the entry limit must trigger a flush");
    assert!(db.get(0u64.to_le_bytes()).unwrap().is_some());
    writer.put(b"large", vec![0; IMPORT_BATCH_BYTES]).unwrap();
    assert!(writer.batch.is_empty(), "one large record must not leave an oversized pending batch");
    writer.put(b"tail", [2]).unwrap();
    assert!(db.get(b"tail").unwrap().is_none());
    writer.flush().unwrap();
    writer.flush().unwrap();
    assert_eq!(db.get(b"tail").unwrap().unwrap(), [2]);
}

/// Same-binary comparison against the original direct-write methods, which
/// remain unchanged. Each arm uses a fresh real RocksDB with WAL enabled.
/// Setup, final equality checks, and database teardown are outside the timer.
#[test]
#[ignore = "manual disk benchmark; run in release mode with --nocapture"]
fn benchmark_import_paths() {
    let values = entries(20_000, 34);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    for mode in ["write", "write_and_muhash", "copy"] {
        let mut samples = [Vec::new(), Vec::new()];
        for round in 0..5 {
            // Alternate arm order to reduce temperature/order bias.
            for active in [round % 2 != 0, round % 2 == 0] {
                let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(32));
                let mut store = DbUtxoSetStore::new(db.clone(), CachePolicy::Count(values.len()), vec![82]);
                let mut multiset = MuHash::new();
                let start = Instant::now();
                for chunk in values.chunks(IMPORT_BATCH_ENTRIES) {
                    if mode == "copy" {
                        let iter = chunk.iter().map(|(o, e)| (*o, Arc::new(e.clone())));
                        if active {
                            store.copy_imported_from_iterator(iter).unwrap();
                        } else {
                            store.write_from_iterator_without_cache(iter).unwrap();
                        }
                    } else {
                        if active {
                            store.write_imported_chunk(chunk).unwrap();
                        } else {
                            store.write_many(chunk).unwrap();
                        }
                        if mode == "write_and_muhash" {
                            let part = pool.install(|| {
                                chunk.par_iter().map(|(o, e)| MuHash::from_utxo(o, e)).reduce(MuHash::new, |mut a, b| {
                                    a.combine(&b);
                                    a
                                })
                            });
                            multiset.combine(&part);
                        }
                    }
                }
                let elapsed = start.elapsed().as_secs_f64();
                samples[usize::from(active)].push(elapsed);
                black_box(multiset.finalize());
                assert_eq!(store.iterator().count(), values.len());
                for (key, entry) in values.iter().step_by(997) {
                    assert_eq!(*UtxoSetStoreReader::get(&store, key).unwrap(), *entry);
                }
            }
        }
        for sample in &mut samples {
            sample.sort_by(f64::total_cmp);
        }
        std::println!(
            "UTXO_IMPORT {mode}: direct={:.3}ms batched={:.3}ms speedup={:.3}x; direct_samples={:?} batched_samples={:?}",
            samples[0][2] * 1000.0,
            samples[1][2] * 1000.0,
            samples[0][2] / samples[1][2],
            samples[0],
            samples[1]
        );
    }
}
