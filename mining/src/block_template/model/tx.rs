pub(crate) struct SelectableTransaction {
    pub(crate) gas_limit: u64,
    pub(crate) p: f64,
}

impl SelectableTransaction {
    pub(crate) fn new(tx_value: f64, gas_limit: u64, alpha: i32) -> Self {
        Self { gas_limit, p: tx_value.powi(alpha) }
    }
}

pub(crate) type SelectableTransactions = Vec<SelectableTransaction>;

pub(crate) type TransactionIndex = usize;

pub(crate) struct Candidate {
    /// SelectableTransaction index in the parent's transactions store
    pub(crate) index: TransactionIndex,

    /// Range start in the candidate list total_p space
    pub(crate) start: f64,

    /// Range end in the candidate list total_p space
    pub(crate) end: f64,

    /// Has this candidate already been selected?
    pub(crate) is_marked_for_deletion: bool,
}

impl Candidate {
    pub(crate) fn new(index: usize, start: f64, end: f64) -> Self {
        Self { index, start, end, is_marked_for_deletion: false }
    }
}

#[derive(Default)]
pub(crate) struct CandidateList {
    pub(crate) candidates: Vec<Candidate>,
    pub(crate) total_p: f64,
}

impl CandidateList {
    pub(crate) fn new(selectable_txs: &SelectableTransactions) -> Self {
        let mut candidates = Vec::with_capacity(selectable_txs.len());
        let mut total_p = 0.0;
        selectable_txs.iter().enumerate().for_each(|(i, tx)| {
            let current_p = tx.p;
            let candidate = Candidate::new(i, total_p, total_p + current_p);
            candidates.push(candidate);
            total_p += current_p;
        });
        Self { candidates, total_p }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.candidates.len() == 0
    }

    pub(crate) fn rebalanced(&self, selectable_txs: &SelectableTransactions) -> Self {
        let mut candidates = Vec::with_capacity(self.candidates.len());
        let mut total_p = 0.0;
        self.candidates.iter().filter(|x| !x.is_marked_for_deletion).for_each(|x| {
            let current_p = selectable_txs[x.index].p;
            let candidate = Candidate::new(x.index, total_p, total_p + current_p);
            candidates.push(candidate);
            total_p += current_p;
        });
        Self { candidates, total_p }
    }

    /// find finds the candidates in whose range r falls.
    /// For example, if we have candidates with starts and ends:
    /// * tx1: start 0,   end 100
    /// * tx2: start 100, end 105
    /// * tx3: start 105, end 2000
    ///
    /// And r=102, then [`CandidateList::find`] will return tx2.
    pub(crate) fn find(&self, r: f64) -> usize {
        let mut min = 0;
        let mut max = self.candidates.len() - 1;
        loop {
            let i = (min + max) / 2;
            let candidate = &self.candidates[i];
            if candidate.end < r {
                min = i + 1;
                continue;
            } else if candidate.start > r {
                max = i - 1;
                continue;
            }
            return i;
        }
    }
}
