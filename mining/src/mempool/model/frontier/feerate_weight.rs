use super::feerate_key::FeerateTransactionKey;
use sweep_bptree::tree::visit::{DescendVisit, DescendVisitResult};
use sweep_bptree::tree::{Argument, SearchArgument};

type FeerateKey = FeerateTransactionKey;

#[derive(Clone, Copy, Debug, Default)]
pub struct FeerateWeight(f64);

impl FeerateWeight {
    /// Returns the weight value
    pub fn weight(&self) -> f64 {
        self.0
    }
}

impl Argument<FeerateKey> for FeerateWeight {
    fn from_leaf(keys: &[FeerateKey]) -> Self {
        Self(keys.iter().map(|k| k.weight()).sum())
    }

    fn from_inner(_keys: &[FeerateKey], arguments: &[Self]) -> Self {
        Self(arguments.iter().map(|a| a.0).sum())
    }
}

impl SearchArgument<FeerateKey> for FeerateWeight {
    type Query = f64;

    fn locate_in_leaf(query: Self::Query, keys: &[FeerateKey]) -> Option<usize> {
        let mut sum = 0.0;
        for (i, k) in keys.iter().enumerate() {
            let w = k.weight();
            sum += w;
            if query < sum {
                return Some(i);
            }
        }
        // In order to avoid sensitivity to floating number arithmetics,
        // we logically "clamp" the search, returning the last leaf if the query
        // value is out of bounds
        match keys.len() {
            0 => None,
            n => Some(n - 1),
        }
    }

    fn locate_in_inner(mut query: Self::Query, _keys: &[FeerateKey], arguments: &[Self]) -> Option<(usize, Self::Query)> {
        for (i, a) in arguments.iter().enumerate() {
            if query >= a.0 {
                query -= a.0;
            } else {
                return Some((i, query));
            }
        }
        // In order to avoid sensitivity to floating number arithmetics,
        // we logically "clamp" the search, returning the last subtree if the query
        // value is out of bounds. Eventually this will lead to the return of the
        // last leaf (see locate_in_leaf as well)
        match arguments.len() {
            0 => None,
            n => Some((n - 1, arguments[n - 1].0)),
        }
    }
}

pub struct PrefixWeightVisitor<'a> {
    key: &'a FeerateKey,
    accumulated_weight: f64,
}

impl<'a> PrefixWeightVisitor<'a> {
    pub fn new(key: &'a FeerateKey) -> Self {
        Self { key, accumulated_weight: Default::default() }
    }

    fn search_in_keys(&self, keys: &[FeerateKey]) -> usize {
        match keys.binary_search(self.key) {
            Err(idx) => {
                // The idx is the place where a matching element could be inserted while maintaining
                // sorted order, go to left child
                idx
            }
            Ok(idx) => {
                // Exact match, go to right child.
                idx + 1
            }
        }
    }
}

impl<'a> DescendVisit<FeerateKey, (), FeerateWeight> for PrefixWeightVisitor<'a> {
    type Result = f64;

    fn visit_inner(&mut self, keys: &[FeerateKey], arguments: &[FeerateWeight]) -> DescendVisitResult<Self::Result> {
        let idx = self.search_in_keys(keys);
        // trace!("[visit_inner] {}, {}, {}", keys.len(), arguments.len(), idx);
        for argument in arguments.iter().take(idx) {
            self.accumulated_weight += argument.weight();
        }
        DescendVisitResult::GoDown(idx)
    }

    fn visit_leaf(&mut self, keys: &[FeerateKey], _values: &[()]) -> Option<Self::Result> {
        let idx = self.search_in_keys(keys);
        // trace!("[visit_leaf] {}, {}", keys.len(), idx);
        for key in keys.iter().take(idx) {
            self.accumulated_weight += key.weight();
        }
        Some(self.accumulated_weight)
    }
}
