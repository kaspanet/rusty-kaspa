use itertools::Itertools;
use kaspa_consensus_core::tx::Transaction;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    iter::{FusedIterator, Map},
};

type IndexSet = HashSet<usize>;

pub trait TopologicalSort {
    fn topological_sort(self) -> Self
    where
        Self: Sized;
}

impl<T: AsRef<Transaction> + Clone> TopologicalSort for Vec<T> {
    fn topological_sort(self) -> Self {
        let mut sorted = Vec::with_capacity(self.len());
        let mut in_degree: Vec<i32> = vec![0; self.len()];

        // Index on transaction ids
        let mut index = HashMap::with_capacity(self.len());
        self.iter().enumerate().for_each(|(idx, tx)| {
            let _ = index.insert(tx.as_ref().id(), idx);
        });

        // Transaction edges
        let mut all_edges: Vec<Option<IndexSet>> = vec![None; self.len()];
        self.iter().enumerate().for_each(|(destination_idx, tx)| {
            tx.as_ref().inputs.iter().for_each(|input| {
                if let Some(origin_idx) = index.get(&input.previous_outpoint.transaction_id) {
                    all_edges[*origin_idx].get_or_insert_with(IndexSet::new).insert(destination_idx);
                }
            })
        });

        // Degrees
        (0..self.len()).for_each(|origin_idx| {
            if let Some(ref edges) = all_edges[origin_idx] {
                edges.iter().for_each(|destination_idx| {
                    in_degree[*destination_idx] += 1;
                });
            }
        });

        // Degree 0
        let mut queue = VecDeque::with_capacity(self.len());
        (0..self.len()).for_each(|destination_idx| {
            if in_degree[destination_idx] == 0 {
                queue.push_back(destination_idx);
            }
        });

        // Sorted transactions
        while !queue.is_empty() {
            let current = queue.pop_front().unwrap();
            if let Some(ref edges) = all_edges[current] {
                edges.iter().for_each(|destination_idx| {
                    let degree = in_degree.get_mut(*destination_idx).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(*destination_idx);
                    }
                });
            }
            sorted.push(self[current].clone());
        }
        assert_eq!(sorted.len(), self.len(), "by definition, cryptographically no cycle can exist in a DAG of transactions");

        sorted
    }
}

pub trait IterTopologically<T>
where
    T: AsRef<Transaction>,
{
    fn topological_iter(&self) -> TopologicalIter<'_, T>;
}

impl<T: AsRef<Transaction>> IterTopologically<T> for &[T] {
    fn topological_iter(&self) -> TopologicalIter<'_, T> {
        TopologicalIter::new(self)
    }
}

impl<T: AsRef<Transaction>> IterTopologically<T> for Vec<T> {
    fn topological_iter(&self) -> TopologicalIter<'_, T> {
        TopologicalIter::new(self)
    }
}

pub struct TopologicalIter<'a, T: AsRef<Transaction>> {
    transactions: &'a [T],
    in_degree: Vec<i32>,
    edges: Vec<Option<IndexSet>>,
    queue: VecDeque<usize>,
    yields_count: usize,
}

impl<'a, T: AsRef<Transaction>> TopologicalIter<'a, T> {
    pub fn new(transactions: &'a [T]) -> Self {
        let mut in_degree: Vec<i32> = vec![0; transactions.len()];

        // Index on transaction ids
        let mut index = HashMap::with_capacity(transactions.len());
        transactions.iter().enumerate().for_each(|(idx, tx)| {
            let _ = index.insert(tx.as_ref().id(), idx);
        });

        // Transaction edges
        let mut edges: Vec<Option<IndexSet>> = vec![None; transactions.len()];
        transactions.iter().enumerate().for_each(|(destination_idx, tx)| {
            tx.as_ref().inputs.iter().for_each(|input| {
                if let Some(origin_idx) = index.get(&input.previous_outpoint.transaction_id) {
                    edges[*origin_idx].get_or_insert_with(IndexSet::new).insert(destination_idx);
                }
            })
        });

        // Degrees
        (0..transactions.len()).for_each(|origin_idx| {
            if let Some(ref edges) = edges[origin_idx] {
                edges.iter().for_each(|destination_idx| {
                    in_degree[*destination_idx] += 1;
                });
            }
        });

        // Degree 0
        let mut queue = VecDeque::with_capacity(transactions.len());
        (0..transactions.len()).for_each(|destination_idx| {
            if in_degree[destination_idx] == 0 {
                queue.push_back(destination_idx);
            }
        });
        Self { transactions, in_degree, edges, queue, yields_count: 0 }
    }
}

impl<'a, T: AsRef<Transaction>> Iterator for TopologicalIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.queue.pop_front() {
            Some(current) => {
                if let Some(ref edges) = self.edges[current] {
                    edges.iter().for_each(|destination_idx| {
                        let degree = self.in_degree.get_mut(*destination_idx).unwrap();
                        *degree -= 1;
                        if *degree == 0 {
                            self.queue.push_back(*destination_idx);
                        }
                    });
                }
                self.yields_count += 1;
                Some(&self.transactions[current])
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let items_remaining = self.transactions.len() - self.yields_count.min(self.transactions.len());
        (self.yields_count, Some(items_remaining))
    }
}

impl<T: AsRef<Transaction>> FusedIterator for TopologicalIter<'_, T> {}
impl<T: AsRef<Transaction>> ExactSizeIterator for TopologicalIter<'_, T> {
    fn len(&self) -> usize {
        self.transactions.len()
    }
}

pub trait IntoIterTopologically<T>
where
    T: AsRef<Transaction>,
{
    fn topological_into_iter(self) -> TopologicalIntoIter<T>;
}

impl<T: AsRef<Transaction>> IntoIterTopologically<T> for Vec<T> {
    fn topological_into_iter(self) -> TopologicalIntoIter<T> {
        TopologicalIntoIter::new(self)
    }
}

impl<T, I, F> IntoIterTopologically<T> for Map<I, F>
where
    T: AsRef<Transaction>,
    I: Iterator,
    F: FnMut(<I as Iterator>::Item) -> T,
{
    fn topological_into_iter(self) -> TopologicalIntoIter<T> {
        TopologicalIntoIter::new(self)
    }
}

pub struct TopologicalIntoIter<T: AsRef<Transaction>> {
    transactions: Vec<Option<T>>,
    in_degree: Vec<i32>,
    edges: Vec<Option<IndexSet>>,
    queue: VecDeque<usize>,
    yields_count: usize,
}

impl<T: AsRef<Transaction>> TopologicalIntoIter<T> {
    pub fn new(transactions: impl IntoIterator<Item = T>) -> Self {
        // Collect all transactions
        let transactions = transactions.into_iter().map(|tx| Some(tx)).collect_vec();

        let mut in_degree: Vec<i32> = vec![0; transactions.len()];

        // Index on transaction ids
        let mut index = HashMap::with_capacity(transactions.len());
        transactions.iter().enumerate().for_each(|(idx, tx)| {
            let _ = index.insert(tx.as_ref().unwrap().as_ref().id(), idx);
        });

        // Transaction edges
        let mut edges: Vec<Option<IndexSet>> = vec![None; transactions.len()];
        transactions.iter().enumerate().for_each(|(destination_idx, tx)| {
            tx.as_ref().unwrap().as_ref().inputs.iter().for_each(|input| {
                if let Some(origin_idx) = index.get(&input.previous_outpoint.transaction_id) {
                    edges[*origin_idx].get_or_insert_with(IndexSet::new).insert(destination_idx);
                }
            })
        });

        // Degrees
        (0..transactions.len()).for_each(|origin_idx| {
            if let Some(ref edges) = edges[origin_idx] {
                edges.iter().for_each(|destination_idx| {
                    in_degree[*destination_idx] += 1;
                });
            }
        });

        // Degree 0
        let mut queue = VecDeque::with_capacity(transactions.len());
        (0..transactions.len()).for_each(|destination_idx| {
            if in_degree[destination_idx] == 0 {
                queue.push_back(destination_idx);
            }
        });
        Self { transactions, in_degree, edges, queue, yields_count: 0 }
    }
}

impl<T: AsRef<Transaction>> Iterator for TopologicalIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self.queue.pop_front() {
            Some(current) => {
                if let Some(ref edges) = self.edges[current] {
                    edges.iter().for_each(|destination_idx| {
                        let degree = self.in_degree.get_mut(*destination_idx).unwrap();
                        *degree -= 1;
                        if *degree == 0 {
                            self.queue.push_back(*destination_idx);
                        }
                    });
                }
                self.yields_count += 1;
                self.transactions[current].take()
            }
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let items_remaining = self.transactions.len() - self.yields_count.min(self.transactions.len());
        (self.yields_count, Some(items_remaining))
    }
}

impl<T: AsRef<Transaction>> FusedIterator for TopologicalIntoIter<T> {}
impl<T: AsRef<Transaction>> ExactSizeIterator for TopologicalIntoIter<T> {
    fn len(&self) -> usize {
        self.transactions.len()
    }
}
