use criterion::{black_box, criterion_group, criterion_main, Criterion};
use itertools::Itertools;
use kaspa_consensus_core::{
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{Transaction, TransactionInput, TransactionOutpoint},
};
use kaspa_hashes::{HasherBase, TransactionID};
use kaspa_mining::{model::topological_index::TopologicalIndex, FeerateTransactionKey, Frontier, Policy};
use rand::{thread_rng, Rng};
use std::{
    collections::{hash_set::Iter, HashMap, HashSet},
    sync::Arc,
};

#[derive(Default)]
pub struct Dag<T>
where
    T: Clone + std::fmt::Debug + std::hash::Hash + Eq,
{
    nodes: HashSet<T>,
    edges: HashMap<T, HashSet<T>>,
}

impl<T> Dag<T>
where
    T: Clone + std::fmt::Debug + std::hash::Hash + Eq,
{
    pub fn add_node(&mut self, key: T) {
        self.nodes.insert(key);
    }

    pub fn add_edge(&mut self, from: T, to: T) {
        self.add_node(from.clone());
        self.add_node(to.clone());
        self.edges.entry(from).or_default().insert(to);
    }
}

impl<'a, T> TopologicalIndex<'a, Iter<'a, T>, Iter<'a, T>, T> for Dag<T>
where
    T: Clone + std::fmt::Debug + std::hash::Hash + Eq,
{
    fn topology_nodes(&'a self) -> Iter<'a, T> {
        self.nodes.iter()
    }

    fn topology_node_edges(&'a self, key: &T) -> Option<Iter<'a, T>> {
        self.edges.get(key).map(|x| x.iter())
    }
}

type Key = &'static str;

fn build_dag() -> Dag<Key> {
    let mut dag: Dag<Key> = Dag::default();
    dag.add_edge("socks", "shoes");
    dag.add_edge("boxer", "shoes");
    dag.add_edge("boxer", "pants");
    dag.add_edge("pants", "belt");
    //dag.add_edge("pants", "shirt");
    dag.add_edge("shirt", "belt");
    dag.add_edge("shirt", "tie");
    dag.add_edge("tie", "jacket");
    dag.add_edge("belt", "jacket");
    dag
}

pub fn bench_compare_topological_index_fns(c: &mut Criterion) {
    let mut group = c.benchmark_group("compare fns");
    group.bench_function("TopologicalIndex::topological_index", |b| {
        let dag = build_dag();
        b.iter(|| (black_box(dag.topological_index())))
    });
    group.bench_function("TopologicalIndex::topological_index_dfs", |b| {
        let dag = build_dag();
        b.iter(|| (black_box(dag.topological_index_dfs())))
    });
    group.finish();
}

fn generate_unique_tx(i: u64) -> Arc<Transaction> {
    let mut hasher = TransactionID::new();
    let prev = hasher.update(i.to_le_bytes()).clone().finalize();
    let input = TransactionInput::new(TransactionOutpoint::new(prev, 0), vec![], 0, 0);
    Arc::new(Transaction::new(0, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]))
}

fn build_feerate_key(fee: u64, mass: u64, id: u64) -> FeerateTransactionKey {
    FeerateTransactionKey::new(fee, mass, generate_unique_tx(id))
}

pub fn bench_mempool_sampling(c: &mut Criterion) {
    let mut rng = thread_rng();
    let mut group = c.benchmark_group("mempool sampling");
    let cap = 1_000_000;
    let mut map = HashMap::with_capacity(cap);
    for i in 0..cap as u64 {
        let fee: u64 = if i % (cap as u64 / 100000) == 0 { 1000000 } else { rng.gen_range(1..10000) };
        let mass: u64 = 1650;
        let key = build_feerate_key(fee, mass, i);
        map.insert(key.tx.id(), key);
    }

    let len = cap;
    let mut frontier = Frontier::default();
    for item in map.values().take(len).cloned() {
        frontier.insert(item).then_some(()).unwrap();
    }
    group.bench_function("mempool one-shot sample", |b| {
        b.iter(|| {
            black_box({
                let selected = frontier.sample_inplace(&mut rng, &Policy::new(500_000), &mut 0);
                selected.iter().map(|k| k.mass).sum::<u64>()
            })
        })
    });

    // Benchmark frontier insertions and removals (see comparisons below)
    let remove = map.values().take(map.len() / 10).cloned().collect_vec();
    group.bench_function("frontier remove/add", |b| {
        b.iter(|| {
            black_box({
                for r in remove.iter() {
                    frontier.remove(r).then_some(()).unwrap();
                }
                for r in remove.iter().cloned() {
                    frontier.insert(r).then_some(()).unwrap();
                }
                0
            })
        })
    });

    // Benchmark hashmap insertions and removals for comparison
    let remove = map.iter().take(map.len() / 10).map(|(&k, v)| (k, v.clone())).collect_vec();
    group.bench_function("map remove/add", |b| {
        b.iter(|| {
            black_box({
                for r in remove.iter() {
                    map.remove(&r.0).unwrap();
                }
                for r in remove.iter().cloned() {
                    map.insert(r.0, r.1.clone());
                }
                0
            })
        })
    });

    // Benchmark std btree set insertions and removals for comparison
    // Results show that frontier (sweep bptree) and std btree set are roughly the same.
    // The slightly higher cost for sweep bptree should be attributed to subtree weight
    // maintenance (see FeerateWeight)
    #[allow(clippy::mutable_key_type)]
    let mut std_btree = std::collections::BTreeSet::from_iter(map.values().cloned());
    let remove = map.iter().take(map.len() / 10).map(|(&k, v)| (k, v.clone())).collect_vec();
    group.bench_function("std btree remove/add", |b| {
        b.iter(|| {
            black_box({
                for (_, key) in remove.iter() {
                    std_btree.remove(key).then_some(()).unwrap();
                }
                for (_, key) in remove.iter() {
                    std_btree.insert(key.clone());
                }
                0
            })
        })
    });
    group.finish();
}

pub fn bench_mempool_selectors(c: &mut Criterion) {
    let mut rng = thread_rng();
    let mut group = c.benchmark_group("mempool selectors");
    let cap = 1_000_000;
    let mut map = HashMap::with_capacity(cap);
    for i in 0..cap as u64 {
        let fee: u64 = rng.gen_range(1..1000000);
        let mass: u64 = 1650;
        let key = build_feerate_key(fee, mass, i);
        map.insert(key.tx.id(), key);
    }

    for len in [100, 300, 350, 500, 1000, 2000, 5000, 10_000, 100_000, 500_000, 1_000_000].into_iter().rev() {
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        group.bench_function(format!("rebalancing selector ({})", len), |b| {
            b.iter(|| {
                black_box({
                    let mut selector = frontier.build_rebalancing_selector();
                    selector.select_transactions().iter().map(|k| k.gas).sum::<u64>()
                })
            })
        });

        let mut collisions = 0;
        let mut n = 0;

        group.bench_function(format!("sample inplace selector ({})", len), |b| {
            b.iter(|| {
                black_box({
                    let mut selector = frontier.build_selector_sample_inplace(&mut collisions);
                    n += 1;
                    selector.select_transactions().iter().map(|k| k.gas).sum::<u64>()
                })
            })
        });

        if n > 0 {
            println!("---------------------- \n  Avg collisions: {}", collisions / n);
        }

        if frontier.total_mass() <= 500_000 {
            group.bench_function(format!("take all selector ({})", len), |b| {
                b.iter(|| {
                    black_box({
                        let mut selector = frontier.build_selector_take_all();
                        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>()
                    })
                })
            });
        }

        group.bench_function(format!("dynamic selector ({})", len), |b| {
            b.iter(|| {
                black_box({
                    let mut selector = frontier.build_selector(&Policy::new(500_000));
                    selector.select_transactions().iter().map(|k| k.gas).sum::<u64>()
                })
            })
        });
    }

    group.finish();
}

pub fn bench_inplace_sampling_worst_case(c: &mut Criterion) {
    let mut group = c.benchmark_group("mempool inplace sampling");
    let max_fee = u64::MAX;
    let fee_steps = (0..10).map(|i| max_fee / 100u64.pow(i)).collect_vec();
    for subgroup_size in [300, 200, 100, 80, 50, 30] {
        let cap = 1_000_000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let fee: u64 = if i < 300 { fee_steps[i as usize / subgroup_size] } else { 1 };
            let mass: u64 = 1650;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        let mut frontier = Frontier::default();
        for item in map.values().cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let mut collisions = 0;
        let mut n = 0;

        group.bench_function(format!("inplace sampling worst case (subgroup size: {})", subgroup_size), |b| {
            b.iter(|| {
                black_box({
                    let mut selector = frontier.build_selector_sample_inplace(&mut collisions);
                    n += 1;
                    selector.select_transactions().iter().map(|k| k.gas).sum::<u64>()
                })
            })
        });

        if n > 0 {
            println!("---------------------- \n  Avg collisions: {}", collisions / n);
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_mempool_sampling,
    bench_mempool_selectors,
    bench_inplace_sampling_worst_case,
    bench_compare_topological_index_fns
);
criterion_main!(benches);
