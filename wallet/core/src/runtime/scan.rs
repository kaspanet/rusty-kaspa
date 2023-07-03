use crate::address::AddressManager;
use crate::imports::*;
use crate::runtime::AtomicBalance;

pub const DEFAULT_WINDOW_SIZE: u32 = 8;

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
    pub window_size: u32,
    pub extent: ScanExtent,
    pub balance: Arc<AtomicBalance>,
    pub current_daa_score: u64,
}

impl Scan {
    pub fn new(address_manager: Arc<AddressManager>, balance: &Arc<AtomicBalance>, current_daa_score: u64) -> Scan {
        Scan {
            address_manager,
            window_size: DEFAULT_WINDOW_SIZE,
            extent: ScanExtent::EmptyWindow,
            balance: balance.clone(),
            current_daa_score,
        }
    }
    pub fn new_with_args(
        address_manager: Arc<AddressManager>,
        window_size: u32,
        extent: ScanExtent,
        balance: &Arc<AtomicBalance>,
        current_daa_score: u64,
    ) -> Scan {
        Scan { address_manager, window_size, extent, balance: balance.clone(), current_daa_score }
    }
}
