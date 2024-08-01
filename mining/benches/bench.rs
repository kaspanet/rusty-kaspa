use criterion::{black_box, criterion_group, criterion_main, Criterion};
use itertools::Itertools;
use kaspa_consensus_core::{
    subnets::SUBNETWORK_ID_NATIVE,
    tx::{Transaction, TransactionId, TransactionInput, TransactionOutpoint},
};
use kaspa_hashes::{HasherBase, TransactionID};
use kaspa_mining::{
    model::{candidate_tx::CandidateTransaction, topological_index::TopologicalIndex},
    FeerateTransactionKey, Frontier, Policy, TransactionsSelector,
};
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

fn stage_two_sampling(
    container: impl IntoIterator<Item = TransactionId>,
    _map: &HashMap<TransactionId, FeerateTransactionKey>,
) -> Vec<Transaction> {
    let tx = generate_unique_tx(u64::MAX);
    let set = container
        .into_iter()
        .map(|_h| {
            // let k = map.get(&h).unwrap();
            CandidateTransaction { calculated_fee: 2500, calculated_mass: 1650, tx: tx.clone() }
        })
        .collect_vec();
    let mut selector = TransactionsSelector::new(Policy::new(500_000), set);
    selector.select_transactions()
}

pub fn bench_two_stage_sampling(c: &mut Criterion) {
    let mut rng = thread_rng();
    let mut group = c.benchmark_group("mempool sampling");
    let cap = 1_000_000;
    let mut map = HashMap::with_capacity(cap);
    for i in 0..cap as u64 {
        let fee: u64 = rng.gen_range(1..100000);
        let mass: u64 = 1650;
        let tx = generate_unique_tx(i);
        map.insert(tx.id(), FeerateTransactionKey { fee: fee.max(mass), mass, id: tx.id() });
    }

    let len = cap;
    let mut frontier = Frontier::default();
    for item in map.values().take(len).cloned() {
        frontier.insert(item).then_some(()).unwrap();
    }
    group.bench_function("mempool sample stage one", |b| {
        b.iter(|| {
            black_box({
                let stage_one = frontier.sample(&mut rng, 10_000);
                stage_one.into_iter().map(|k| k.as_bytes()[0] as u64).sum::<u64>()
            })
        })
    });
    group.bench_function("mempool sample stage one & two", |b| {
        b.iter(|| {
            black_box({
                let stage_one = frontier.sample(&mut rng, 10_000);
                let stage_two = stage_two_sampling(stage_one, &map);
                stage_two.into_iter().map(|k| k.gas).sum::<u64>()
            })
        })
    });
    group.finish();
}

criterion_group!(benches, bench_two_stage_sampling, bench_compare_topological_index_fns);
criterion_main!(benches);
