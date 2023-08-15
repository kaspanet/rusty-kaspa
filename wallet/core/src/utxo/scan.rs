use crate::derivation::AddressManager;
use crate::imports::*;
use crate::result::Result;
use crate::runtime::{AtomicBalance, Balance};
use crate::utxo::{UtxoContext, UtxoEntryReference};
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

pub struct Scan {
    pub address_manager: Arc<AddressManager>,
    pub window_size: Option<usize>,
    pub extent: ScanExtent,
    pub balance: Arc<AtomicBalance>,
    pub current_daa_score: u64,
}

impl Scan {
    pub fn new(address_manager: Arc<AddressManager>, balance: &Arc<AtomicBalance>, current_daa_score: u64) -> Scan {
        Scan {
            address_manager,
            window_size: Some(DEFAULT_WINDOW_SIZE),
            extent: ScanExtent::EmptyWindow,
            balance: balance.clone(),
            current_daa_score,
        }
    }
    pub fn new_with_args(
        address_manager: Arc<AddressManager>,
        window_size: Option<usize>,
        extent: ScanExtent,
        balance: &Arc<AtomicBalance>,
        current_daa_score: u64,
    ) -> Scan {
        // let window_size = window_size.unwrap_or(DEFAULT_WINDOW_SIZE);
        Scan { address_manager, window_size, extent, balance: balance.clone(), current_daa_score }
    }

    // pub async fn scan(self: &Arc<Self>, utxo_context: &Arc<UtxoContext>) -> Result<()> {
    pub async fn scan(&self, utxo_context: &UtxoContext) -> Result<()> {
        let window_size = self.window_size.unwrap_or(DEFAULT_WINDOW_SIZE) as u32;

        let mut cursor: u32 = 0;
        let mut last_address_index = self.address_manager.index();

        'scan: loop {
            // scan first up to address index, then in window chunks
            let first = cursor;
            let last = if cursor == 0 { max(last_address_index + 1, window_size) } else { cursor + window_size };
            cursor = last;
            // log_info!("first: {}, last: {}", first, last);

            // generate address derivations
            let addresses = self.address_manager.get_range(first..last).await?;
            // register address in the utxo context; NOTE:  during the scan,
            // before `get_utxos_by_addresses()` is complete we may receive
            // new transactions  as such utxo context should be aware of the
            // addresses used before we start interacting with them.
            utxo_context.register_addresses(&addresses).await?;

            let ts = Instant::now();
            let resp = utxo_context.processor().rpc().get_utxos_by_addresses(addresses).await?;
            let elapsed_msec = ts.elapsed().as_secs_f32();
            if elapsed_msec > 1.0 {
                log_warning!("get_utxos_by_address() fetched {} entries in: {} msec", resp.len(), elapsed_msec);
            }
            yield_executor().await;

            let refs: Vec<UtxoEntryReference> = resp.into_iter().map(UtxoEntryReference::from).collect();
            for utxo_ref in refs.iter() {
                if let Some(address) = utxo_ref.utxo.address.as_ref() {
                    if let Some(utxo_address_index) = self.address_manager.inner().address_to_index_map.get(address) {
                        if last_address_index < *utxo_address_index {
                            last_address_index = *utxo_address_index;
                        }
                    } else {
                        panic!("Account::scan_address_manager() has received an unknown address: `{address}`");
                    }
                }
            }
            yield_executor().await;

            let balance: Balance = refs.iter().fold(Balance::default(), |mut balance, r| {
                let entry_balance = r.as_ref().balance(self.current_daa_score);
                balance.mature += entry_balance.mature;
                balance.pending += entry_balance.pending;
                balance
            });
            yield_executor().await;

            utxo_context.extend(refs, self.current_daa_score).await?;

            if !balance.is_empty() {
                self.balance.add(balance);
            } else {
                match &self.extent {
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
        self.address_manager.set_index(last_address_index)?;

        Ok(())
    }
}
