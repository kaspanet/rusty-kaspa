use criterion::{black_box, criterion_group, criterion_main, Criterion};
use itertools::Itertools;
use kaspa_consensus_core::{
    muhash::MuHashExtensions,
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{ScriptPublicKey, SignableTransaction, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
};
use kaspa_hashes::TransactionID;
use kaspa_muhash::MuHash;
use kaspa_utils::iter::parallelism_in_power_steps;
use rayon::prelude::*;

fn generate_transaction(ins: usize, outs: usize, randomness: u64) -> SignableTransaction {
    let mut tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
    let mut entries = vec![];
    for i in 0..ins {
        let mut hasher = TransactionID::new();
        hasher.write(i.to_le_bytes());
        hasher.write(randomness.to_le_bytes());
        let input = TransactionInput::new(TransactionOutpoint::new(hasher.finalize(), 0), vec![10; 66], 0, 1);
        let entry = UtxoEntry::new(22222222, ScriptPublicKey::from_vec(0, vec![99; 34]), 23456, false);
        tx.inputs.push(input);
        entries.push(entry);
    }
    for _ in 0..outs {
        let output = TransactionOutput::new(23456, ScriptPublicKey::from_vec(0, vec![101; 34]));
        tx.outputs.push(output);
    }
    tx.finalize();
    SignableTransaction::with_entries(tx, entries)
}

pub fn parallel_muhash_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("muhash txs");
    let txs = (0..256).map(|i| generate_transaction(2, 2, i)).collect_vec();
    group.bench_function("seq", |b| {
        b.iter(|| {
            let mut mh = MuHash::new();
            for tx in txs.iter() {
                mh.add_transaction(&tx.as_verifiable(), 222);
            }
            black_box(mh)
        })
    });

    for threads in parallelism_in_power_steps() {
        group.bench_function(format!("par {threads}"), |b| {
            let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap();
            b.iter(|| {
                pool.install(|| {
                    let mh =
                        txs.par_iter().map(|tx| MuHash::from_transaction(&tx.as_verifiable(), 222)).reduce(MuHash::new, |mut a, b| {
                            a.combine(&b);
                            a
                        });
                    black_box(mh)
                })
            })
        });
    }

    group.finish();
}

criterion_group!(benches, parallel_muhash_benchmark);
criterion_main!(benches);
