use std::collections::{HashMap, HashSet};

use kaspa_hashes::Hash;
use kaspa_rpc_core::RpcUtxosByAddressesEntry;

/// Tracks L1 UTXO balances for monitored addresses.
///
/// Fed by `UtxosChanged` notifications and initial `get_utxos_by_addresses` fetches.
#[derive(Default)]
pub struct UtxoTracker {
    /// Address string -> list of live UTXOs (outpoint, amount).
    confirmed: HashMap<String, Vec<Utxo>>,
    /// Outpoints consumed by submitted-but-unconfirmed transactions.
    pending_spent: HashSet<(Hash, u32)>,
}

#[derive(Clone, Debug)]
pub struct Utxo {
    pub tx_id: Hash,
    pub index: u32,
    pub amount: u64,
}

impl UtxoTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Bulk-load initial UTXOs from `get_utxos_by_addresses`.
    pub fn load_initial(&mut self, entries: &[RpcUtxosByAddressesEntry]) {
        for entry in entries {
            let addr = match &entry.address {
                Some(a) => a.to_string(),
                None => continue,
            };
            let utxo = Utxo { tx_id: entry.outpoint.transaction_id, index: entry.outpoint.index, amount: entry.utxo_entry.amount };
            self.confirmed.entry(addr).or_default().push(utxo);
        }
    }

    /// Process a UtxosChanged notification.
    pub fn apply_utxos_changed(&mut self, added: &[RpcUtxosByAddressesEntry], removed: &[RpcUtxosByAddressesEntry]) {
        // Remove spent UTXOs
        for entry in removed {
            let addr = match &entry.address {
                Some(a) => a.to_string(),
                None => continue,
            };
            if let Some(utxos) = self.confirmed.get_mut(&addr) {
                utxos.retain(|u| u.tx_id != entry.outpoint.transaction_id || u.index != entry.outpoint.index);
            }
            // Clear from pending_spent if it was there
            self.pending_spent.remove(&(entry.outpoint.transaction_id, entry.outpoint.index));
        }

        // Add new UTXOs
        for entry in added {
            let addr = match &entry.address {
                Some(a) => a.to_string(),
                None => continue,
            };
            let utxo = Utxo { tx_id: entry.outpoint.transaction_id, index: entry.outpoint.index, amount: entry.utxo_entry.amount };
            self.confirmed.entry(addr).or_default().push(utxo);
        }
    }

    /// Mark an outpoint as spent (pending confirmation).
    pub fn mark_spent(&mut self, tx_id: Hash, index: u32) {
        self.pending_spent.insert((tx_id, index));
    }

    /// Get confirmed balance for an address (excluding pending_spent).
    pub fn balance(&self, address: &str) -> u64 {
        self.confirmed
            .get(address)
            .map(|utxos| utxos.iter().filter(|u| !self.pending_spent.contains(&(u.tx_id, u.index))).map(|u| u.amount).sum())
            .unwrap_or(0)
    }

    /// Get all UTXOs for an address (excluding pending_spent).
    pub fn available_utxos(&self, address: &str) -> Vec<&Utxo> {
        self.confirmed
            .get(address)
            .map(|utxos| utxos.iter().filter(|u| !self.pending_spent.contains(&(u.tx_id, u.index))).collect())
            .unwrap_or_default()
    }

    /// Simple greedy UTXO selection. Returns selected UTXOs and total value.
    pub fn select_utxos(&self, address: &str, target: u64) -> Option<(Vec<Utxo>, u64)> {
        let available = self.available_utxos(address);
        let mut selected = Vec::new();
        let mut total = 0u64;
        for utxo in available {
            selected.push(utxo.clone());
            total += utxo.amount;
            if total >= target {
                return Some((selected, total));
            }
        }
        None // Insufficient funds
    }

    /// Clear pending_spent state (call on restart before re-fetching UTXOs).
    pub fn clear(&mut self) {
        self.confirmed.clear();
        self.pending_spent.clear();
    }
}
