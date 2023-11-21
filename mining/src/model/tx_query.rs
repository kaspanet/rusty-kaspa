/// Indicates whether the mempool query result should include transactions/orphans or both
pub enum TransactionQuery {
    /// Include only non-orphan transactions from the ordinary mempool tx pool
    TransactionsOnly,
    /// Include orphan transactions only
    OrphansOnly,
    /// Include both orphan and non-orphan transactions
    All,
}

impl TransactionQuery {
    pub fn include_transaction_pool(&self) -> bool {
        matches!(self, TransactionQuery::TransactionsOnly | TransactionQuery::All)
    }

    pub fn include_orphan_pool(&self) -> bool {
        matches!(self, TransactionQuery::OrphansOnly | TransactionQuery::All)
    }
}
