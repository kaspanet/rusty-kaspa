use criterion::{black_box, criterion_group, criterion_main, Criterion};
use mining::model::topological_index::TopologicalIndex;
use std::collections::{hash_set::Iter, HashMap, HashSet};

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
    pub fn new() -> Self {
        Self { nodes: HashSet::default(), edges: HashMap::default() }
    }

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
    let mut dag: Dag<Key> = Dag::new();
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

criterion_group!(benches, bench_compare_topological_index_fns);
criterion_main!(benches);
