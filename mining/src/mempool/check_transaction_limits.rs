use crate::mempool::{
    Mempool,
    errors::{RuleError, RuleResult},
};
use kaspa_consensus_core::{mass::NonContextualMasses, tx::MutableTransaction};

impl Mempool {
    /// Validates non-contextual transaction dimensions against the consensus block limits.
    ///
    /// This is intentionally separate from standardness: even when non-standard transactions are accepted,
    /// the mempool must not admit a transaction which selectors can never include in a block. The transaction
    /// is expected to have its non-contextual masses populated before this call. These checks run before
    /// consensus in-context validation so transactions above compute/transient limits do not reach script execution.
    pub(crate) fn validate_transaction_limits_in_isolation(
        &self,
        transaction: &MutableTransaction,
        virtual_daa_score: u64,
    ) -> RuleResult<()> {
        if transaction.tx.gas > self.config.block_lane_limits.gas_per_lane {
            return Err(RuleError::RejectGas(transaction.id(), transaction.tx.gas, self.config.block_lane_limits.gas_per_lane));
        }

        let limits = self.config.mempool_block_mass_limits.get(virtual_daa_score);
        let NonContextualMasses { compute_mass, transient_mass } = transaction.calculated_non_contextual_masses.unwrap();
        if compute_mass > limits.compute {
            return Err(RuleError::RejectComputeMass(transaction.id(), compute_mass, limits.compute));
        }
        if transient_mass > limits.transient {
            return Err(RuleError::RejectTransientMass(transaction.id(), transient_mass, limits.transient));
        }

        Ok(())
    }

    /// Validates contextual transaction dimensions against the consensus block limits.
    ///
    /// This is intentionally separate from standardness: even when non-standard transactions are accepted,
    /// the mempool must not admit a transaction which selectors can never include in a block. The transaction
    /// is expected to have contextual storage mass populated by consensus validation before this call.
    pub(crate) fn validate_transaction_limits_in_context(
        &self,
        transaction: &MutableTransaction,
        virtual_daa_score: u64,
    ) -> RuleResult<()> {
        let limits = self.config.mempool_block_mass_limits.get(virtual_daa_score);
        let storage_mass = transaction.tx.storage_mass();
        if storage_mass > limits.storage {
            return Err(RuleError::RejectStorageMass(transaction.id(), storage_mass, limits.storage));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MiningCounters, mempool::config::Config};
    use kaspa_consensus_core::{
        config::{constants::consensus::DEFAULT_LANES_PER_BLOCK_LIMIT, params::ForkActivation},
        constants::{MAX_TX_IN_SEQUENCE_NUM, SOMPI_PER_KASPA, TX_VERSION},
        mass::{BlockLaneLimits, BlockMassLimits},
        subnets::SUBNETWORK_ID_NATIVE,
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry, scriptvec},
    };
    use kaspa_hashes::Hash;
    use std::sync::Arc;

    const LIMITS: BlockMassLimits = BlockMassLimits { compute: 100, storage: 200, transient: 300 };
    const GAS_PER_LANE: u64 = 7;

    enum Expected {
        Ok,
        RejectGas(u64, u64),
        RejectComputeMass(u64, u64),
        RejectTransientMass(u64, u64),
        RejectStorageMass(u64, u64),
    }

    struct Test {
        name: &'static str,
        gas: u64,
        compute_mass: u64,
        transient_mass: u64,
        storage_mass: u64,
        expected: Expected,
    }

    fn mempool() -> Mempool {
        let config = Config::build_default(
            100,
            true,
            LIMITS,
            BlockLaneLimits { lanes_per_block: DEFAULT_LANES_PER_BLOCK_LIMIT, gas_per_lane: GAS_PER_LANE },
        );
        Mempool::new(Arc::new(config), ForkActivation::never(), Arc::new(MiningCounters::default()))
    }

    fn transaction(gas: u64, compute_mass: u64, transient_mass: u64, storage_mass: u64) -> MutableTransaction {
        let script_public_key = ScriptPublicKey::new(0, scriptvec![0x51]);
        let outpoint = TransactionOutpoint::new(Hash::from_u64_word(1), 0);
        let input = TransactionInput::new(outpoint, vec![], MAX_TX_IN_SEQUENCE_NUM, 0);
        let output = TransactionOutput::new(SOMPI_PER_KASPA, script_public_key.clone());
        let tx = Transaction::new(TX_VERSION, vec![input], vec![output], 0, SUBNETWORK_ID_NATIVE, gas, vec![]);
        tx.set_storage_mass(storage_mass);
        let entry = UtxoEntry::new(SOMPI_PER_KASPA, script_public_key, 0, false, None);
        let mut tx = MutableTransaction::with_entries(tx.into(), vec![entry]);
        tx.calculated_non_contextual_masses = Some(NonContextualMasses::new(compute_mass, transient_mass));
        tx
    }

    fn assert_expected(name: &str, result: RuleResult<()>, tx: &MutableTransaction, expected: Expected) {
        let expected = match expected {
            Expected::Ok => Ok(()),
            Expected::RejectGas(gas, limit) => Err(RuleError::RejectGas(tx.id(), gas, limit)),
            Expected::RejectComputeMass(mass, limit) => Err(RuleError::RejectComputeMass(tx.id(), mass, limit)),
            Expected::RejectTransientMass(mass, limit) => Err(RuleError::RejectTransientMass(tx.id(), mass, limit)),
            Expected::RejectStorageMass(mass, limit) => Err(RuleError::RejectStorageMass(tx.id(), mass, limit)),
        };
        assert_eq!(result, expected, "failed for test '{name}'");
    }

    #[test]
    fn test_validate_transaction_limits_in_isolation() {
        let tests = [
            Test {
                name: "non-contextual values at limits",
                gas: GAS_PER_LANE,
                compute_mass: LIMITS.compute,
                transient_mass: LIMITS.transient,
                storage_mass: 0,
                expected: Expected::Ok,
            },
            Test {
                name: "transaction gas exceeds the per-lane limit",
                gas: GAS_PER_LANE + 1,
                compute_mass: 1,
                transient_mass: 1,
                storage_mass: 0,
                expected: Expected::RejectGas(GAS_PER_LANE + 1, GAS_PER_LANE),
            },
            Test {
                name: "transaction compute mass exceeds the block limit",
                gas: 0,
                compute_mass: LIMITS.compute + 1,
                transient_mass: 1,
                storage_mass: 0,
                expected: Expected::RejectComputeMass(LIMITS.compute + 1, LIMITS.compute),
            },
            Test {
                name: "transaction transient byte size exceeds the block limit",
                gas: 0,
                compute_mass: 1,
                transient_mass: LIMITS.transient + 1,
                storage_mass: 0,
                expected: Expected::RejectTransientMass(LIMITS.transient + 1, LIMITS.transient),
            },
        ];

        let mempool = mempool();
        for test in tests {
            let tx = transaction(test.gas, test.compute_mass, test.transient_mass, test.storage_mass);
            let result = mempool.validate_transaction_limits_in_isolation(&tx, 0);
            assert_expected(test.name, result, &tx, test.expected);
        }
    }

    #[test]
    fn test_validate_transaction_limits_in_context() {
        let tests = [
            Test {
                name: "contextual storage value at limit",
                gas: 0,
                compute_mass: 1,
                transient_mass: 1,
                storage_mass: LIMITS.storage,
                expected: Expected::Ok,
            },
            Test {
                name: "transaction storage mass exceeds the block limit",
                gas: 0,
                compute_mass: 1,
                transient_mass: 1,
                storage_mass: LIMITS.storage + 1,
                expected: Expected::RejectStorageMass(LIMITS.storage + 1, LIMITS.storage),
            },
        ];

        let mempool = mempool();
        for test in tests {
            let tx = transaction(test.gas, test.compute_mass, test.transient_mass, test.storage_mass);
            let result = mempool.validate_transaction_limits_in_context(&tx, 0);
            assert_expected(test.name, result, &tx, test.expected);
        }
    }
}
