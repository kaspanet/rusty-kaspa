use std::collections::{HashMap, HashSet, VecDeque};

pub trait TopologicalIndex<'a, TNSet, TESet, TKey>
where
    TNSet: Iterator<Item = &'a TKey> + ExactSizeIterator,
    TESet: Iterator<Item = &'a TKey> + ExactSizeIterator,
    TKey: Clone + std::hash::Hash + Eq + 'a,
{
    /// Returns the set of node keys.
    fn topology_nodes(&'a self) -> TNSet;

    /// Returns the set of edges from a key to other nodes.
    fn topology_node_edges(&'a self, key: &TKey) -> Option<TESet>;

    /// Returns a topologically ordered index of the node keys.
    ///
    /// The implementation bases on Kahn's in-degree algorithm.
    fn topological_index(&'a self) -> TopologicalIndexResult<Vec<TKey>> {
        let mut sorted = Vec::with_capacity(self.topology_nodes().len());
        let mut in_degree: HashMap<TKey, u32> = HashMap::with_capacity(self.topology_nodes().len());
        self.topology_nodes().for_each(|key| {
            in_degree.insert(key.clone(), 0);
        });

        self.topology_nodes().for_each(|key| {
            if let Some(edges) = self.topology_node_edges(key) {
                edges.for_each(|node| {
                    *in_degree.get_mut(node).unwrap() += 1;
                });
            }
        });

        let mut queue = VecDeque::with_capacity(self.topology_nodes().len());
        in_degree.iter().for_each(|(key, degree)| {
            if *degree == 0 {
                queue.push_back(key.clone());
            }
        });

        while !queue.is_empty() {
            let current = queue.pop_front().unwrap();
            if let Some(edges) = self.topology_node_edges(&current) {
                edges.for_each(|node| {
                    let degree = in_degree.get_mut(node).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(node.clone());
                    }
                });
            }
            sorted.push(current);
        }

        if sorted.len() != self.topology_nodes().len() {
            Err(TopologicalIndexError::HasCycle)
        } else {
            Ok(sorted)
        }
    }

    /// Returns a topologically ordered index of the node keys.
    ///
    /// The implementation bases on DFS iterative with color algorithm.
    fn topological_index_dfs(&'a self) -> TopologicalIndexResult<Vec<TKey>> {
        #[derive(Clone, PartialEq, Debug)]
        enum Color {
            White = 0,
            Gray = 1,
            Black = 2,
        }
        let mut color = HashMap::with_capacity(self.topology_nodes().len());
        self.topology_nodes().for_each(|key| {
            color.insert(key.clone(), Color::White);
        });
        let mut stack = Vec::with_capacity(self.topology_nodes().len());
        let mut sorted = Vec::with_capacity(self.topology_nodes().len());

        for key in self.topology_nodes() {
            if color[key] != Color::White {
                continue;
            }
            stack.push(key.clone());

            while let Some(current) = stack.pop() {
                let current_color = color.get_mut(&current).unwrap();
                match *current_color {
                    Color::White => {
                        *current_color = Color::Gray;
                        stack.push(current.clone());
                    }
                    Color::Gray => {
                        *current_color = Color::Black;
                        sorted.push(current.clone());
                    }
                    Color::Black => {}
                }
                if let Some(edges) = self.topology_node_edges(&current) {
                    for node in edges {
                        match color.get(node).unwrap() {
                            Color::White => {
                                stack.push(node.clone());
                            }
                            Color::Gray => {
                                return Err(TopologicalIndexError::HasCycle);
                            }
                            Color::Black => {}
                        }
                    }
                }
            }
        }

        sorted.reverse();
        Ok(sorted)
    }

    fn check_topological_order(&'a self, sorted: &[TKey]) -> TopologicalIndexResult<()> {
        let nodes = self.topology_nodes().collect::<HashSet<_>>();
        if sorted.len() != nodes.len() {
            return Err(TopologicalIndexError::IndexHasWrongKeySet);
        }
        let mut key_index = HashMap::new();
        for (i, key) in sorted.iter().enumerate() {
            if key_index.insert(key.clone(), i).is_some() {
                return Err(TopologicalIndexError::IndexHasNonUniqueKey);
            }
            if !nodes.contains(key) {
                return Err(TopologicalIndexError::IndexHasWrongKeySet);
            }
        }
        for from_key in self.topology_nodes() {
            if let Some(edges) = self.topology_node_edges(from_key) {
                for to_key in edges {
                    if key_index[from_key] > key_index[to_key] {
                        return Err(TopologicalIndexError::IndexIsNotTopological);
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum TopologicalIndexError {
    HasCycle,

    IndexHasNonUniqueKey,
    IndexHasWrongKeySet,
    IndexIsNotTopological,
}

pub type TopologicalIndexResult<T> = Result<T, TopologicalIndexError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_set::Iter;

    struct Dag<T>
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
        fn new() -> Self {
            Self { nodes: HashSet::default(), edges: HashMap::default() }
        }

        fn add_node(&mut self, key: T) {
            self.nodes.insert(key);
        }

        fn add_edge(&mut self, from: T, to: T) {
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

    fn build_dag(with_cycle: bool) -> Dag<Key> {
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
        if with_cycle {
            dag.add_edge("jacket", "boxer");
        }
        dag
    }

    #[test]
    fn test_topological_index() {
        struct Test {
            name: &'static str,
            dag: Dag<Key>,
            is_acyclic: bool,
        }

        let tests = vec![
            Test { name: "a regular DAG", dag: build_dag(false), is_acyclic: true },
            Test { name: "an invalid DAG with one cycle", dag: build_dag(true), is_acyclic: false },
        ];

        for test in tests.iter() {
            let index_in_degree = test.dag.topological_index();
            assert_eq!(
                index_in_degree.is_ok(),
                test.is_acyclic,
                "testing {}, expecting the in-degree index to be {}",
                test.name,
                if test.is_acyclic { "acyclic" } else { "invalid" }
            );
            if let Ok(ref index) = index_in_degree {
                let test_result = test.dag.check_topological_order(index);
                assert!(
                    test_result.is_ok(),
                    "testing {}, expecting {:?} to be topologically ordered and got {:?}",
                    test.name,
                    index_in_degree,
                    test_result
                );
            }

            let index_dfs = test.dag.topological_index_dfs();
            assert_eq!(
                index_dfs.is_ok(),
                test.is_acyclic,
                "testing {}, expecting the dfs index to be {}",
                test.name,
                if test.is_acyclic { "acyclic" } else { "invalid" }
            );
            if let Ok(ref index) = index_dfs {
                let test_result = test.dag.check_topological_order(index);
                assert!(
                    test_result.is_ok(),
                    "testing {}, expecting {:?} to be topologically ordered and got {:?}",
                    test.name,
                    index_dfs,
                    test_result
                );
            }
        }
    }

    #[test]
    fn test_check_topological_order() {
        struct Test {
            name: &'static str,
            index: Vec<Key>,
            expected_result: TopologicalIndexResult<()>,
        }

        let tests = vec![
            Test {
                name: "topologically ordered index",
                index: vec!["shirt", "socks", "tie", "boxer", "pants", "belt", "jacket", "shoes"],
                expected_result: Ok(()),
            },
            Test {
                name: "index has duplicate key",
                index: vec!["shirt", "shirt", "tie", "boxer", "pants", "belt", "jacket", "shoes"],
                expected_result: Err(TopologicalIndexError::IndexHasNonUniqueKey),
            },
            Test {
                name: "index has a wrong set of keys",
                index: vec!["UNKNOWN", "shirt", "tie", "boxer", "pants", "belt", "jacket", "shoes"],
                expected_result: Err(TopologicalIndexError::IndexHasWrongKeySet),
            },
            Test {
                name: "index has a shorter set of keys",
                index: vec!["shirt", "tie", "boxer"],
                expected_result: Err(TopologicalIndexError::IndexHasWrongKeySet),
            },
            Test {
                name: "index is not topologically ordered",
                index: vec!["jacket", "socks", "tie", "belt", "pants", "shirt", "boxer", "shoes"],
                expected_result: Err(TopologicalIndexError::IndexIsNotTopological),
            },
        ];

        for test in tests.iter() {
            let dag = build_dag(false);
            let result = dag.check_topological_order(&test.index);
            assert_eq!(
                result, test.expected_result,
                "testing '{}', expecting {:?} but got {:?}",
                test.name, test.expected_result, result
            );
        }
    }
}
