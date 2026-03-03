use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE;
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput};
use kaspa_hashes::Hash;
use kaspa_txscript::{pay_to_address_script, pay_to_script_hash_script};
use zk_covenant_rollup_host::bridge::build_delegate_entry_script;
use zk_covenant_rollup_host::mock_tx::{find_action_tx_nonce, find_entry_tx_nonce, find_exit_tx_nonce};

use crate::balance::Utxo;
use crate::db::Pubkey;

/// Compute fee for a transaction given its mass and priority feerate.
/// Returns at least the minimum relay fee.
pub fn compute_fee(mass: u64, priority_feerate: f64) -> u64 {
    let fee = (mass as f64 * priority_feerate).ceil() as u64;
    fee.max(mass).max(1000)
}

/// Convert a Hash (32 bytes LE) to [u32; 8] (LE words) for host crate compatibility.
pub fn hash_to_u32x8(h: Hash) -> [u32; 8] {
    let bytes = h.as_bytes();
    let mut words = [0u32; 8];
    for (i, word) in words.iter_mut().enumerate() {
        *word = u32::from_le_bytes(bytes[i * 4..(i + 1) * 4].try_into().unwrap());
    }
    words
}

/// Build an unsigned V1 entry (deposit) transaction.
///
/// The tx_id will have the entry prefix after nonce grinding.
/// Output[0] sends `deposit_amount` to P2SH(delegate_entry_script(covenant_id)).
pub fn build_entry_tx(
    dest_pk: Pubkey,
    covenant_id: Hash,
    deposit_amount: u64,
    gas_utxo: &Utxo,
    fee: u64,
) -> Result<Transaction, String> {
    let required = deposit_amount + fee;
    if gas_utxo.amount < required {
        return Err(format!("UTXO value {} too small for deposit {} + fee {}", gas_utxo.amount, deposit_amount, fee));
    }

    let dest = hash_to_u32x8(dest_pk);

    // Build delegate entry script for this covenant
    let delegate_script = build_delegate_entry_script(&covenant_id.as_bytes());
    let deposit_spk = pay_to_script_hash_script(&delegate_script);

    // Outputs: deposit to covenant + change back to sender (fee subtracted)
    let change = gas_utxo.amount - deposit_amount - fee;
    let mut outputs = vec![TransactionOutput::new(deposit_amount, deposit_spk)];
    if change > 0 {
        let dest_addr = Address::new(Prefix::Testnet, Version::PubKey, &dest_pk.as_bytes());
        outputs.push(TransactionOutput::new(change, pay_to_address_script(&dest_addr)));
    }

    // Find nonce that produces entry-prefix tx_id
    let inputs = vec![TransactionInput::new(TransactionOutpoint::new(gas_utxo.tx_id, gas_utxo.index), vec![], 0, 0)];
    let payload = find_entry_tx_nonce(dest, &inputs, &outputs);

    Ok(Transaction::new(
        1, // V1
        inputs,
        outputs,
        0,
        SUBNETWORK_ID_NATIVE,
        0,
        payload.as_bytes(),
    ))
}

/// Build an unsigned V1 transfer action transaction.
///
/// The tx_id will have the action prefix after nonce grinding.
pub fn build_transfer_tx(
    source_pk: Pubkey,
    dest_pk: Pubkey,
    amount: u64,
    gas_utxo: &Utxo,
    dest_address: &Address,
    fee: u64,
) -> Result<Transaction, String> {
    let output_value =
        gas_utxo.amount.checked_sub(fee).ok_or_else(|| format!("UTXO value {} too small for fee {}", gas_utxo.amount, fee))?;

    let source = hash_to_u32x8(source_pk);
    let dest = hash_to_u32x8(dest_pk);

    let outputs = vec![TransactionOutput::new(output_value, pay_to_address_script(dest_address))];
    let inputs = vec![TransactionInput::new(TransactionOutpoint::new(gas_utxo.tx_id, gas_utxo.index), vec![], 0, 0)];

    let payload = find_action_tx_nonce(source, dest, amount, &inputs, &outputs);

    Ok(Transaction::new(1, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes()))
}

/// Build an unsigned V1 exit (withdrawal) transaction.
///
/// The tx_id will have the exit prefix after nonce grinding.
pub fn build_exit_tx(
    source_pk: Pubkey,
    exit_amount: u64,
    dest_spk_bytes: &[u8],
    gas_utxo: &Utxo,
    fee: u64,
) -> Result<Transaction, String> {
    let output_value =
        gas_utxo.amount.checked_sub(fee).ok_or_else(|| format!("UTXO value {} too small for fee {}", gas_utxo.amount, fee))?;

    let source = hash_to_u32x8(source_pk);

    let outputs = vec![TransactionOutput::new(output_value, kaspa_consensus_core::tx::ScriptPublicKey::new(0, dest_spk_bytes.into()))];
    let inputs = vec![TransactionInput::new(TransactionOutpoint::new(gas_utxo.tx_id, gas_utxo.index), vec![], 0, 0)];

    let payload = find_exit_tx_nonce(source, dest_spk_bytes, exit_amount, &inputs, &outputs);

    Ok(Transaction::new(1, inputs, outputs, 0, SUBNETWORK_ID_NATIVE, 0, payload.as_bytes()))
}
