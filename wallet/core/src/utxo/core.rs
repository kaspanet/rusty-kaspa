use super::{PendingUtxoEntryReference, UtxoEntryId};
use crate::imports::*;
use crate::result::Result;
// use js_sys::BigInt;
use crate::runtime::Account;
use crate::storage::AccountId;
use crate::utxo::UtxoProcessorContext;
use std::collections::HashMap;

#[derive(Default, Clone)]
pub struct UtxoProcessorCore {
    pub pending: Arc<DashMap<UtxoEntryId, PendingUtxoEntryReference>>,
    // pub address_to_account_map : Arc<Mutex<HashMap<Address, Arc<Account>>>>
    pub address_to_account_map: Arc<DashMap<Address, Arc<Account>>>,
}

impl UtxoProcessorCore {
    pub fn address_to_account_map(&self) -> &Arc<DashMap<Address, Arc<Account>>> {
        &self.address_to_account_map
    }

    pub fn address_to_account(&self, address: &Address) -> Option<Arc<Account>> {
        self.address_to_account_map.get(address).map(|v| v.clone())
    }

    pub fn register_addresses(&self, addresses: Vec<Address>, account: &Arc<Account>) {
        addresses.into_iter().for_each(|address| {
            self.address_to_account_map.insert(address, account.clone());
        })
    }

    pub fn unregister_addresses(&self, addresses: Vec<Address>) {
        addresses.into_iter().for_each(|address| {
            self.address_to_account_map.remove(&address);
        })
    }

    pub fn create_context(&self, account: &Arc<Account>) -> UtxoProcessorContext {
        UtxoProcessorContext { core: self.clone(), account: account.clone() }
    }

    pub async fn handle_pending(&self, current_daa_score: u64) -> Result<Vec<Arc<Account>>> {
        let mature_entries = {
            let mut mature_entries = vec![];
            let pending_entries = &self.pending;
            pending_entries.retain(|_, pending| {
                if pending.is_mature(current_daa_score) {
                    mature_entries.push(pending.clone());
                    false
                } else {
                    true
                }
            });
            mature_entries
        };

        let mut accounts = HashMap::<AccountId, Arc<Account>>::default();
        for mature in mature_entries.into_iter() {
            let account = mature.account;
            let entry = mature.entry;
            // account.handle_utxo_matured(entry).await?;

            // account.utxo_db().promote(utxo);
            account.utxo_processor().promote(entry);

            accounts.insert(*account.id(), account.clone());
        }

        let accounts = accounts.values().cloned().collect::<Vec<_>>();
        Ok(accounts)
        // for (_, account) in accounts.into_iter() {
        //     account.update_balance().await?;
        // }

        // Ok(())
    }
}
