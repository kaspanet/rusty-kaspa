use crate::stubs::ScriptClass;

use super::{
    errors::{NonStandardError, NonStandardResult},
    Mempool,
};
use consensus_core::{
    constants::{MAX_SCRIPT_PUBLIC_KEY_VERSION, MAX_SOMPI},
    mass,
    tx::{MutableTransaction, TransactionOutput},
};

/// MAX_STANDARD_P2SH_SIG_OPS is the maximum number of signature operations
/// that are considered standard in a pay-to-script-hash script.
const MAX_STANDARD_P2SH_SIG_OPS: u8 = 15;

/// MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE is the maximum size allowed for a
/// transaction input signature script to be considered standard. This
/// value allows for a 15-of-15 CHECKMULTISIG pay-to-script-hash with
/// compressed keys.
///
/// The form of the overall script is: OP_0 <15 signatures> OP_PUSHDATA2
/// <2 bytes len> [OP_15 <15 pubkeys> OP_15 OP_CHECKMULTISIG]
///
/// For the p2sh script portion, each of the 15 compressed pubkeys are
/// 33 bytes (plus one for the OP_DATA_33 opcode), and the thus it totals
/// to (15*34)+3 = 513 bytes. Next, each of the 15 signatures is a max
/// of 73 bytes (plus one for the OP_DATA_73 opcode). Also, there is one
/// extra byte for the initial extra OP_0 push and 3 bytes for the
/// OP_PUSHDATA2 needed to specify the 513 bytes for the script push.
/// That brings the total to 1+(15*74)+3+513 = 1627. This value also
/// adds a few extra bytes to provide a little buffer.
/// (1 + 15*74 + 3) + (15*34 + 3) + 23 = 1650
const MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE: u64 = 1650;

/// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum mass allowed for transactions that
/// are considered standard and will therefore be relayed and considered for mining.
const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 100_000;

impl Mempool {
    pub(crate) fn check_transaction_standard_in_isolation(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        // The transaction must be a currently supported version.
        //
        // This check is currently mirrored in consensus.
        // However, in a later version of Kaspa the consensus-valid transaction version range might diverge from the
        // standard transaction version range, and thus the validation should happen in both levels.
        if transaction.tx.version > self.config.maximum_standard_transaction_version
            || transaction.tx.version < self.config.minimum_standard_transaction_version
        {
            return Err(NonStandardError::RejectVersion(
                transaction_id,
                transaction.tx.version,
                self.config.minimum_standard_transaction_version,
                self.config.maximum_standard_transaction_version,
            ));
        }

        // Since extremely large transactions with a lot of inputs can cost
        // almost as much to process as the sender fees, limit the maximum
        // size of a transaction. This also helps mitigate CPU exhaustion
        // attacks.
        if transaction.calculated_mass.unwrap() > MAXIMUM_STANDARD_TRANSACTION_MASS {
            return Err(NonStandardError::RejectMass(
                transaction_id,
                transaction.calculated_mass.unwrap(),
                MAXIMUM_STANDARD_TRANSACTION_MASS,
            ));
        }

        for (i, input) in transaction.tx.inputs.iter().enumerate() {
            // Each transaction input signature script must not exceed the
            // maximum size allowed for a standard transaction.
            //
            // See the comment on MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE for
            // more details.
            let signature_script_len = input.signature_script.len() as u64;
            if signature_script_len > MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE {
                return Err(NonStandardError::RejectSignatureScriptSize(
                    transaction_id,
                    i,
                    signature_script_len,
                    MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE,
                ));
            }
        }

        // None of the output public key scripts can be a non-standard script or be "dust".
        for (i, output) in transaction.tx.outputs.iter().enumerate() {
            if output.script_public_key.version() > MAX_SCRIPT_PUBLIC_KEY_VERSION {
                return Err(NonStandardError::RejectScriptPublicKeyVersion(transaction_id, i));
            }

            // TODO: call script engine when available
            // script_class = txscript.get_script_class(output.script_public_key.script())
            let script_class = ScriptClass::PubKey;
            if script_class == ScriptClass::NonStandard {
                return Err(NonStandardError::RejectOutputScriptClass(transaction_id, i));
            }

            if self.is_transaction_output_dust(output) {
                return Err(NonStandardError::RejectDust(transaction_id, i, output.value));
            }
        }

        Ok(())
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    ///
    /// It is exposed by [MiningManager] for use by transaction generators and wallets.
    pub(crate) fn is_transaction_output_dust(&self, transaction_output: &TransactionOutput) -> bool {
        // Unspendable outputs are considered dust.
        //
        // TODO: call script engine when available
        // if txscript.is_unspendable(transaction_output.script_public_key.script()) {
        //     return true
        // }
        //

        // The total serialized size consists of the output and the associated
        // input script to redeem it. Since there is no input script
        // to redeem it yet, use the minimum size of a typical input script.
        //
        // Pay-to-pubkey bytes breakdown:
        //
        //  Output to pubkey (43 bytes):
        //   8 value, 1 script len, 34 script [1 OP_DATA_32,
        //   32 pubkey, 1 OP_CHECKSIG]
        //
        //  Input (105 bytes):
        //   36 prev outpoint, 1 script len, 64 script [1 OP_DATA_64,
        //   64 sig], 4 sequence
        //
        // The most common scripts are pay-to-pubkey, and as per the above
        // breakdown, the minimum size of a p2pk input script is 148 bytes. So
        // that figure is used.
        let total_serialized_size = mass::transaction_output_estimated_serialized_size(transaction_output) + 148;

        // The output is considered dust if the cost to the network to spend the
        // coins is more than 1/3 of the minimum free transaction relay fee.
        // mp.config.MinimumRelayTransactionFee is in sompi/KB, so multiply
        // by 1000 to convert to bytes.
        //
        // Using the typical values for a pay-to-pubkey transaction from
        // the breakdown above and the default minimum free transaction relay
        // fee of 1000, this equates to values less than 546 sompi being
        // considered dust.
        //
        // The following is equivalent to (value/total_serialized_size) * (1/3) * 1000
        // without needing to do floating point math.
        transaction_output.value * 1000 / (3 * total_serialized_size) < self.config.minimum_relay_transaction_fee
    }

    /// check_transaction_standard_in_context performs a series of checks on a transaction's
    /// inputs to ensure they are "standard". A standard transaction input within the
    /// context of this function is one whose referenced public key script is of a
    /// standard form and, for pay-to-script-hash, does not have more than
    /// maxStandardP2SHSigOps signature operations.
    /// In addition, makes sure that the transaction's fee is above the minimum for acceptance
    /// into the mempool and relay.
    pub(crate) fn check_transaction_standard_in_context(&self, transaction: &MutableTransaction) -> NonStandardResult<()> {
        let transaction_id = transaction.id();

        for (i, _input) in transaction.tx.inputs.iter().enumerate() {
            // It is safe to elide existence and index checks here since
            // they have already been checked prior to calling this
            // function.

            // TODO: call script engine when available
            // let entry = transaction.entries[i].as_ref().unwrap();
            // let origin_script_key = entry.script_public_key.script();
            // script_class = txscript.get_script_class(origin_script_key)
            let script_class = ScriptClass::ScriptHash;
            match script_class {
                ScriptClass::NonStandard => {
                    return Err(NonStandardError::RejectInputScriptClass(transaction_id, i));
                }

                // TODO: handle these 2 cases
                ScriptClass::PubKey => {}
                ScriptClass::_PubKeyECDSA => {}

                ScriptClass::ScriptHash => {
                    // TODO: call script engine when available
                    //  txscript.GetPreciseSigOpCount(input.SignatureScript, origin_script_key, true)
                    let num_sig_ops = 1;
                    if num_sig_ops > MAX_STANDARD_P2SH_SIG_OPS {
                        return Err(NonStandardError::RejectSignatureCount(transaction_id, i, num_sig_ops, MAX_STANDARD_P2SH_SIG_OPS));
                    }
                }
            }

            let minimum_fee = self.minimum_required_transaction_relay_fee(transaction.calculated_mass.unwrap());
            if transaction.calculated_fee.unwrap() < minimum_fee {
                return Err(NonStandardError::RejectInsufficientFee(transaction_id, transaction.calculated_fee.unwrap(), minimum_fee));
            }
        }

        Ok(())
    }

    /// minimum_required_transaction_relay_fee returns the minimum transaction fee required
    /// for a transaction with the passed mass to be accepted into the mempool and relayed.
    fn minimum_required_transaction_relay_fee(&self, mass: u64) -> u64 {
        // Calculate the minimum fee for a transaction to be allowed into the
        // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
        // sompi/kg so multiply by mass (which is in grams) and divide by 1000 to get
        // minimum sompis.
        let mut minimum_fee = (mass * self.config.minimum_relay_transaction_fee) / 1000;

        if minimum_fee == 0 {
            minimum_fee = self.config.minimum_relay_transaction_fee;
        }

        // Set the minimum fee to the maximum possible value if the calculated
        // fee is not in the valid range for monetary amounts.
        minimum_fee = minimum_fee.min(MAX_SOMPI);

        minimum_fee
    }
}
