use crate::constants::{MAX_SOMPI, TX_VERSION};
use kaspa_consensus_core::tx::Transaction;
use std::collections::HashSet;

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};

impl TransactionValidator {
    /// Performs a variety of transaction validation checks which are independent of any
    /// context -- header or utxo. **Note** that any check performed here should be moved to
    /// header contextual validation if it becomes HF activation dependent. This is bcs we rely
    /// on checks here to be truly independent and avoid calling it multiple times wherever possible
    /// (e.g., BBT relies on mempool in isolation checks even though virtual daa score might have changed)   
    pub fn validate_tx_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_inputs_in_isolation(tx)?;
        self.check_transaction_outputs_in_isolation(tx)?;
        self.check_coinbase_in_isolation(tx)?;

        check_transaction_output_value_ranges(tx)?;
        check_duplicate_transaction_inputs(tx)?;
        check_gas(tx)?;
        check_transaction_subnetwork(tx)?;
        check_transaction_version(tx)
    }

    fn check_transaction_inputs_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_inputs_count(tx)?;
        self.check_transaction_signature_scripts(tx)
    }

    fn check_transaction_outputs_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        self.check_transaction_outputs_count(tx)?;
        self.check_transaction_script_public_keys(tx)
    }

    fn check_coinbase_in_isolation(&self, tx: &Transaction) -> TxResult<()> {
        if !tx.is_coinbase() {
            return Ok(());
        }
        if !tx.inputs.is_empty() {
            return Err(TxRuleError::CoinbaseHasInputs(tx.inputs.len()));
        }

        if tx.mass() > 0 {
            return Err(TxRuleError::CoinbaseNonZeroMassCommitment);
        }

        let outputs_limit = self.ghostdag_k as u64 + 2;
        if tx.outputs.len() as u64 > outputs_limit {
            return Err(TxRuleError::CoinbaseTooManyOutputs(tx.outputs.len(), outputs_limit));
        }

        for (i, output) in tx.outputs.iter().enumerate() {
            if output.script_public_key.script().len() > self.coinbase_payload_script_public_key_max_len as usize {
                return Err(TxRuleError::CoinbaseScriptPublicKeyTooLong(i));
            }
        }
        Ok(())
    }

    fn check_transaction_outputs_count(&self, tx: &Transaction) -> TxResult<()> {
        if tx.is_coinbase() {
            // We already check coinbase outputs count vs. Ghostdag K + 2
            return Ok(());
        }
        if tx.outputs.len() > self.max_tx_outputs.after() {
            return Err(TxRuleError::TooManyOutputs(tx.outputs.len(), self.max_tx_inputs.after()));
        }

        Ok(())
    }

    fn check_transaction_inputs_count(&self, tx: &Transaction) -> TxResult<()> {
        if !tx.is_coinbase() && tx.inputs.is_empty() {
            return Err(TxRuleError::NoTxInputs);
        }

        if tx.inputs.len() > self.max_tx_inputs.after() {
            return Err(TxRuleError::TooManyInputs(tx.inputs.len(), self.max_tx_inputs.after()));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_signature_scripts(&self, tx: &Transaction) -> TxResult<()> {
        if let Some(i) = tx.inputs.iter().position(|input| input.signature_script.len() > self.max_signature_script_len.after()) {
            return Err(TxRuleError::TooBigSignatureScript(i, self.max_signature_script_len.after()));
        }

        Ok(())
    }

    // The main purpose of this check is to avoid overflows when calculating transaction mass later.
    fn check_transaction_script_public_keys(&self, tx: &Transaction) -> TxResult<()> {
        if let Some(i) =
            tx.outputs.iter().position(|out| out.script_public_key.script().len() > self.max_script_public_key_len.after())
        {
            return Err(TxRuleError::TooBigScriptPublicKey(i, self.max_script_public_key_len.after()));
        }

        Ok(())
    }
}

fn check_duplicate_transaction_inputs(tx: &Transaction) -> TxResult<()> {
    let mut existing = HashSet::new();
    for input in &tx.inputs {
        if !existing.insert(input.previous_outpoint) {
            return Err(TxRuleError::TxDuplicateInputs);
        }
    }
    Ok(())
}

fn check_gas(tx: &Transaction) -> TxResult<()> {
    // This should be revised if subnetworks are activated (along with other validations that weren't copied from kaspad)
    if tx.gas > 0 {
        return Err(TxRuleError::TxHasGas);
    }
    Ok(())
}

fn check_transaction_version(tx: &Transaction) -> TxResult<()> {
    if tx.version != TX_VERSION {
        return Err(TxRuleError::UnknownTxVersion(tx.version));
    }
    Ok(())
}

fn check_transaction_output_value_ranges(tx: &Transaction) -> TxResult<()> {
    let mut total: u64 = 0;
    for (i, output) in tx.outputs.iter().enumerate() {
        if output.value == 0 {
            return Err(TxRuleError::TxOutZero(i));
        }

        if output.value > MAX_SOMPI {
            return Err(TxRuleError::TxOutTooHigh(i));
        }

        if let Some(new_total) = total.checked_add(output.value) {
            total = new_total
        } else {
            return Err(TxRuleError::OutputsValueOverflow);
        }

        if total > MAX_SOMPI {
            return Err(TxRuleError::TotalTxOutTooHigh);
        }
    }

    Ok(())
}

fn check_transaction_subnetwork(tx: &Transaction) -> TxResult<()> {
    if tx.is_coinbase() || tx.subnetwork_id.is_native() {
        Ok(())
    } else {
        Err(TxRuleError::SubnetworksDisabled(tx.subnetwork_id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{
        subnets::{SubnetworkId, SUBNETWORK_ID_COINBASE, SUBNETWORK_ID_NATIVE},
        tx::{scriptvec, ScriptPublicKey, Transaction, TransactionId, TransactionInput, TransactionOutpoint, TransactionOutput},
    };
    use kaspa_core::assert_match;

    use crate::{
        constants::TX_VERSION,
        params::MAINNET_PARAMS,
        processes::transaction_validator::{errors::TxRuleError, TransactionValidator},
    };

    #[test]
    fn validate_tx_in_isolation_test() {
        let mut params = MAINNET_PARAMS.clone();
        params.prior_max_tx_inputs = 10;
        params.prior_max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.prior_max_tx_inputs,
            params.prior_max_tx_outputs,
            params.prior_max_signature_script_len,
            params.prior_max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.prior_coinbase_maturity,
            params.ghostdag_k().after(),
            Default::default(),
        );

        let valid_cb = Transaction::new(
            0,
            vec![],
            vec![TransactionOutput {
                value: 0x12a05f200,
                script_public_key: ScriptPublicKey::new(
                    0,
                    scriptvec!(
                        0xa9, 0x14, 0xda, 0x17, 0x45, 0xe9, 0xb5, 0x49, 0xbd, 0x0b, 0xfa, 0x1a, 0x56, 0x99, 0x71, 0xc7, 0x7e, 0xba,
                        0x30, 0xcd, 0x5a, 0x4b, 0x87
                    ),
                ),
            }],
            0,
            SUBNETWORK_ID_COINBASE,
            0,
            vec![9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        );

        tv.validate_tx_in_isolation(&valid_cb).unwrap();

        let valid_tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint {
                    transaction_id: TransactionId::from_slice(&[
                        0x03, 0x2e, 0x38, 0xe9, 0xc0, 0xa8, 0x4c, 0x60, 0x46, 0xd6, 0x87, 0xd1, 0x05, 0x56, 0xdc, 0xac, 0xc4, 0x1d,
                        0x27, 0x5e, 0xc5, 0x5f, 0xc0, 0x07, 0x79, 0xac, 0x88, 0xfd, 0xf3, 0x57, 0xa1, 0x87,
                    ]),
                    index: 0,
                },
                signature_script: vec![
                    0x49, // OP_DATA_73
                    0x30, 0x46, 0x02, 0x21, 0x00, 0xc3, 0x52, 0xd3, 0xdd, 0x99, 0x3a, 0x98, 0x1b, 0xeb, 0xa4, 0xa6, 0x3a, 0xd1, 0x5c,
                    0x20, 0x92, 0x75, 0xca, 0x94, 0x70, 0xab, 0xfc, 0xd5, 0x7d, 0xa9, 0x3b, 0x58, 0xe4, 0xeb, 0x5d, 0xce, 0x82, 0x02,
                    0x21, 0x00, 0x84, 0x07, 0x92, 0xbc, 0x1f, 0x45, 0x60, 0x62, 0x81, 0x9f, 0x15, 0xd3, 0x3e, 0xe7, 0x05, 0x5c, 0xf7,
                    0xb5, 0xee, 0x1a, 0xf1, 0xeb, 0xcc, 0x60, 0x28, 0xd9, 0xcd, 0xb1, 0xc3, 0xaf, 0x77, 0x48,
                    0x01, // 73-byte signature
                    0x41, // OP_DATA_65
                    0x04, 0xf4, 0x6d, 0xb5, 0xe9, 0xd6, 0x1a, 0x9d, 0xc2, 0x7b, 0x8d, 0x64, 0xad, 0x23, 0xe7, 0x38, 0x3a, 0x4e, 0x6c,
                    0xa1, 0x64, 0x59, 0x3c, 0x25, 0x27, 0xc0, 0x38, 0xc0, 0x85, 0x7e, 0xb6, 0x7e, 0xe8, 0xe8, 0x25, 0xdc, 0xa6, 0x50,
                    0x46, 0xb8, 0x2c, 0x93, 0x31, 0x58, 0x6c, 0x82, 0xe0, 0xfd, 0x1f, 0x63, 0x3f, 0x25, 0xf8, 0x7c, 0x16, 0x1b, 0xc6,
                    0xf8, 0xa6, 0x30, 0x12, 0x1d, 0xf2, 0xb3, 0xd3, // 65-byte pubkey
                ],
                sequence: u64::MAX,
                sig_op_count: 0,
            }],
            vec![
                TransactionOutput {
                    value: 0x2123e300,
                    script_public_key: ScriptPublicKey::new(
                        0,
                        scriptvec!(
                            0x76, // OP_DUP
                            0xa9, // OP_HASH160
                            0x14, // OP_DATA_20
                            0xc3, 0x98, 0xef, 0xa9, 0xc3, 0x92, 0xba, 0x60, 0x13, 0xc5, 0xe0, 0x4e, 0xe7, 0x29, 0x75, 0x5e, 0xf7,
                            0xf5, 0x8b, 0x32, 0x88, // OP_EQUALVERIFY
                            0xac  // OP_CHECKSIG
                        ),
                    ),
                },
                TransactionOutput {
                    value: 0x108e20f00,
                    script_public_key: ScriptPublicKey::new(
                        0,
                        scriptvec!(
                            0x76, // OP_DUP
                            0xa9, // OP_HASH160
                            0x14, // OP_DATA_20
                            0x94, 0x8c, 0x76, 0x5a, 0x69, 0x14, 0xd4, 0x3f, 0x2a, 0x7a, 0xc1, 0x77, 0xda, 0x2c, 0x2f, 0x6b, 0x52,
                            0xde, 0x3d, 0x7c, 0x88, // OP_EQUALVERIFY
                            0xac  // OP_CHECKSIG
                        ),
                    ),
                },
            ],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        tv.validate_tx_in_isolation(&valid_tx).unwrap();

        let mut tx: Transaction = valid_tx.clone();
        tx.subnetwork_id = SubnetworkId::from_byte(3);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::SubnetworksDisabled(_)));

        let mut tx = valid_tx.clone();
        tx.inputs = vec![];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::NoTxInputs));

        let mut tx = valid_tx.clone();
        tx.inputs = (0..params.prior_max_tx_inputs + 1).map(|_| valid_tx.inputs[0].clone()).collect();
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooManyInputs(_, _)));

        let mut tx = valid_tx.clone();
        tx.inputs[0].signature_script = vec![0; params.prior_max_signature_script_len + 1];
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooBigSignatureScript(_, _)));

        let mut tx = valid_tx.clone();
        tx.outputs = (0..params.prior_max_tx_outputs + 1).map(|_| valid_tx.outputs[0].clone()).collect();
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooManyOutputs(_, _)));

        let mut tx = valid_tx.clone();
        tx.outputs[0].script_public_key = ScriptPublicKey::new(0, scriptvec![0u8; params.prior_max_script_public_key_len + 1]);
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TooBigScriptPublicKey(_, _)));

        let mut tx = valid_tx.clone();
        tx.inputs.push(tx.inputs[0].clone());
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TxDuplicateInputs));

        let mut tx = valid_tx.clone();
        tx.gas = 1;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::TxHasGas));

        let mut tx = valid_tx.clone();
        tx.payload = vec![0];
        assert_match!(tv.validate_tx_in_isolation(&tx), Ok(()));

        let mut tx = valid_tx;
        tx.version = TX_VERSION + 1;
        assert_match!(tv.validate_tx_in_isolation(&tx), Err(TxRuleError::UnknownTxVersion(_)));
    }
}
