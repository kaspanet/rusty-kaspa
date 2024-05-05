//!
//! Address scanner implementation, responsible for
//! aggregating UTXOs from multiple addresses and
//! building corresponding balances.
//!

use crate::derivation::AddressManager;
use crate::imports::*;
use crate::utxo::balance::AtomicBalance;
use crate::utxo::{UtxoContext, UtxoEntryReference, UtxoEntryReferenceExtension};
use std::cmp::max;

pub const DEFAULT_WINDOW_SIZE: usize = 8;

#[derive(Default, Clone, Copy)]
pub enum ScanExtent {
    /// Scan until an empty range is found
    #[default]
    EmptyWindow,
    /// Scan until a specific depth (a particular derivation index)
    Depth(u32),
}

enum Provider {
    AddressManager(Arc<AddressManager>),
    AddressSet(HashSet<Address>),
}

pub struct Scan {
    provider: Provider,
    window_size: Option<usize>,
    extent: Option<ScanExtent>,
    balance: Arc<AtomicBalance>,
    current_daa_score: u64,
}

impl Scan {
    pub fn new_with_address_manager(
        address_manager: Arc<AddressManager>,
        balance: &Arc<AtomicBalance>,
        current_daa_score: u64,
        window_size: Option<usize>,
        extent: Option<ScanExtent>,
    ) -> Scan {
        Scan { provider: Provider::AddressManager(address_manager), window_size, extent, balance: balance.clone(), current_daa_score }
    }
    pub fn new_with_address_set(addresses: HashSet<Address>, balance: &Arc<AtomicBalance>, current_daa_score: u64) -> Scan {
        Scan {
            provider: Provider::AddressSet(addresses),
            window_size: None,
            extent: None,
            balance: balance.clone(),
            current_daa_score,
        }
    }

    pub async fn scan(&self, utxo_context: &UtxoContext) -> Result<()> {
        match &self.provider {
            Provider::AddressManager(address_manager) => self.scan_with_address_manager(address_manager, utxo_context).await,
            Provider::AddressSet(addresses) => self.scan_with_address_set(addresses, utxo_context).await,
        }
    }

    pub async fn scan_with_address_manager(&self, address_manager: &Arc<AddressManager>, utxo_context: &UtxoContext) -> Result<()> {
        let params = utxo_context.processor().network_params()?;

        let window_size = self.window_size.unwrap_or(DEFAULT_WINDOW_SIZE) as u32;
        let extent = self.extent.expect("address manager requires an extent");

        let mut cursor: u32 = 0;
        let mut last_address_index = address_manager.index();

        'scan: loop {
            // scan first up to address index, then in window chunks
            let first = cursor;
            let last = if cursor == 0 { max(last_address_index + 1, window_size) } else { cursor + window_size };
            cursor = last;

            // generate address derivations
            let addresses = address_manager.get_range(first..last)?;
            // register address in the utxo context; NOTE:  during the scan,
            // before `get_utxos_by_addresses()` is complete we may receive
            // new transactions  as such utxo context should be aware of the
            // addresses used before we start interacting with them.
            utxo_context.register_addresses(&addresses).await?;

            let ts = Instant::now();
            let resp = utxo_context.processor().rpc_api().get_utxos_by_addresses(addresses).await?;
            let elapsed_msec = ts.elapsed().as_secs_f32();
            if elapsed_msec > 1.0 {
                log_warn!("get_utxos_by_address() fetched {} entries in: {} msec", resp.len(), elapsed_msec);
            }
            yield_executor().await;

            if !resp.is_empty() {
                let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
                for utxo_ref in refs.iter() {
                    if let Some(address) = utxo_ref.utxo.address.as_ref() {
                        if let Some(utxo_address_index) = address_manager.inner().address_to_index_map.get(address) {
                            if last_address_index < *utxo_address_index {
                                last_address_index = *utxo_address_index;
                            }
                        } else {
                            panic!("Account::scan_address_manager() has received an unknown address: `{address}`");
                        }
                    }
                }

                let balance: Balance = refs.iter().fold(Balance::default(), |mut balance, r| {
                    let entry_balance = r.balance(params, self.current_daa_score);
                    balance.mature += entry_balance.mature;
                    balance.pending += entry_balance.pending;
                    balance.mature_utxo_count += entry_balance.mature_utxo_count;
                    balance.pending_utxo_count += entry_balance.pending_utxo_count;
                    balance.stasis_utxo_count += entry_balance.stasis_utxo_count;
                    balance
                });

                utxo_context.extend_from_scan(refs, self.current_daa_score).await?;

                self.balance.add(balance);
            } else {
                match &extent {
                    ScanExtent::EmptyWindow => {
                        if cursor > last_address_index + window_size {
                            break 'scan;
                        }
                    }
                    ScanExtent::Depth(depth) => {
                        if &cursor > depth {
                            break 'scan;
                        }
                    }
                }
            }
            yield_executor().await;
        }

        // update address manager with the last used index
        address_manager.set_index(last_address_index)?;

        Ok(())
    }

    pub async fn scan_with_address_set(&self, address_set: &HashSet<Address>, utxo_context: &UtxoContext) -> Result<()> {
        let params = utxo_context.processor().network_params()?;
        let address_vec = address_set.iter().cloned().collect::<Vec<_>>();

        utxo_context.register_addresses(&address_vec).await?;
        let resp = utxo_context.processor().rpc_api().get_utxos_by_addresses(address_vec).await?;
        let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();

        let balance: Balance = refs.iter().fold(Balance::default(), |mut balance, r| {
            let entry_balance = r.balance(params, self.current_daa_score);
            balance.mature += entry_balance.mature;
            balance.pending += entry_balance.pending;
            balance.mature_utxo_count += entry_balance.mature_utxo_count;
            balance.pending_utxo_count += entry_balance.pending_utxo_count;
            balance.stasis_utxo_count += entry_balance.stasis_utxo_count;
            balance
        });
        yield_executor().await;

        utxo_context.extend_from_scan(refs, self.current_daa_score).await?;

        if !balance.is_empty() {
            self.balance.add(balance);
        }

        Ok(())
    }
}
