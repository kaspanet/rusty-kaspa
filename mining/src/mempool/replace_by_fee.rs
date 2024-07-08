use crate::mempool::{
    errors::{RuleError, RuleResult},
    model::tx::{DoubleSpend, MempoolTransaction, RbfPolicy, TxRemovalReason},
    Mempool,
};
use kaspa_consensus_core::tx::{MutableTransaction, Transaction};
use std::sync::Arc;

impl Mempool {
    /// Returns the replace by fee (RBF) constraint fee/mass threshold for an incoming transaction and a policy.
    ///
    /// Fails if the transaction does not meet some condition of the RBF policy, excluding the fee/mass condition.
    ///
    /// See [`RbfPolicy`] variants for details of each policy process and success conditions.
    pub(super) fn get_replace_by_fee_constraint(
        &self,
        transaction: &mut MutableTransaction,
        rbf_policy: RbfPolicy,
    ) -> RuleResult<Option<f64>> {
        match rbf_policy {
            RbfPolicy::Forbidden => {
                // When RBF is forbidden, fails early on any double spend
                self.transaction_pool.check_double_spends(transaction)?;
                Ok(None)
            }

            RbfPolicy::Allowed => {
                // When RBF is allowed, never fails since both insertion and replacement are possible
                let double_spends = self.transaction_pool.get_double_spend_transaction_ids(transaction);
                if double_spends.is_empty() {
                    Ok(None)
                } else {
                    let fee_per_mass_threshold = self.get_double_spend_fee_per_mass(&double_spends[0])?;
                    Ok(Some(fee_per_mass_threshold))
                }
            }

            RbfPolicy::Mandatory => {
                // When RBF is mandatory, fails early if we do not have exactly one double spending transaction
                let double_spends = self.transaction_pool.get_double_spend_transaction_ids(transaction);
                match double_spends.len() {
                    0 => Err(RuleError::RejectRbfNoDoubleSpend),
                    1 => {
                        let fee_per_mass_threshold = self.get_double_spend_fee_per_mass(&double_spends[0])?;
                        Ok(Some(fee_per_mass_threshold))
                    }
                    _ => Err(RuleError::RejectRbfTooManyDoubleSpendingTransactions),
                }
            }
        }
    }

    /// Executes replace by fee (RBF) for an incoming transaction and a policy.
    ///
    /// See [`RbfPolicy`] variants for details of each policy process and success conditions.
    ///
    /// On success, `transaction` is guaranteed to embed no double spend with the mempool.
    ///
    /// On success with the [`RbfPolicy::Mandatory`] policy, some removed transaction is always returned.
    pub(super) fn execute_replace_by_fee(
        &mut self,
        transaction: &MutableTransaction,
        rbf_policy: RbfPolicy,
    ) -> RuleResult<Option<Arc<Transaction>>> {
        match rbf_policy {
            RbfPolicy::Forbidden => {
                self.transaction_pool.check_double_spends(transaction)?;
                Ok(None)
            }

            RbfPolicy::Allowed => {
                let double_spends = self.transaction_pool.get_double_spend_transaction_ids(transaction);
                match double_spends.is_empty() {
                    true => Ok(None),
                    false => {
                        let removed = self.validate_double_spending_transaction(transaction, &double_spends[0])?.mtx.tx.clone();
                        for double_spend in double_spends {
                            self.remove_transaction(
                                &double_spend.owner_id,
                                true,
                                TxRemovalReason::ReplacedByFee,
                                format!("by {}", transaction.id()).as_str(),
                            )?;
                        }
                        Ok(Some(removed))
                    }
                }
            }

            RbfPolicy::Mandatory => {
                let double_spends = self.transaction_pool.get_double_spend_transaction_ids(transaction);
                match double_spends.len() {
                    0 => Err(RuleError::RejectRbfNoDoubleSpend),
                    1 => {
                        let removed = self.validate_double_spending_transaction(transaction, &double_spends[0])?.mtx.tx.clone();
                        self.remove_transaction(
                            &double_spends[0].owner_id,
                            true,
                            TxRemovalReason::ReplacedByFee,
                            format!("by {}", transaction.id()).as_str(),
                        )?;
                        Ok(Some(removed))
                    }
                    _ => Err(RuleError::RejectRbfTooManyDoubleSpendingTransactions),
                }
            }
        }
    }

    fn get_double_spend_fee_per_mass(&self, double_spend: &DoubleSpend) -> RuleResult<f64> {
        let owner = self.transaction_pool.get_double_spend_owner(double_spend)?;
        match owner.mtx.calculated_fee_per_compute_mass() {
            Some(double_spend_ratio) => Ok(double_spend_ratio),
            None => Err(double_spend.into()),
        }
    }

    fn validate_double_spending_transaction<'a>(
        &'a self,
        transaction: &MutableTransaction,
        double_spend: &DoubleSpend,
    ) -> RuleResult<&'a MempoolTransaction> {
        let owner = self.transaction_pool.get_double_spend_owner(double_spend)?;
        if let (Some(transaction_ratio), Some(double_spend_ratio)) =
            (transaction.calculated_fee_per_compute_mass(), owner.mtx.calculated_fee_per_compute_mass())
        {
            if transaction_ratio > double_spend_ratio {
                return Ok(owner);
            }
        }
        Err(double_spend.into())
    }
}
