use crate::address::tracker::Tracker;
use std::{ops::Deref, sync::Arc};

#[cfg(test)]
use kaspa_addresses::Address;

#[derive(Debug, Default)]
pub struct SubscriptionContextInner {
    pub address_tracker: Tracker,
}

impl SubscriptionContextInner {
    pub fn new() -> Self {
        let address_tracker = Tracker::new();
        Self { address_tracker }
    }

    #[cfg(test)]
    pub fn with_addresses(addresses: &[Address]) -> Self {
        let address_tracker = Tracker::with_addresses(addresses);
        Self { address_tracker }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SubscriptionContext {
    inner: Arc<SubscriptionContextInner>,
}

impl SubscriptionContext {
    pub fn new() -> Self {
        let inner = Arc::new(SubscriptionContextInner::new());
        Self { inner }
    }

    #[cfg(test)]
    pub fn with_addresses(addresses: &[Address]) -> Self {
        let inner = Arc::new(SubscriptionContextInner::with_addresses(addresses));
        Self { inner }
    }
}

impl Deref for SubscriptionContext {
    type Target = SubscriptionContextInner;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use itertools::Itertools;
    use kaspa_addresses::{Address, Prefix};
    use kaspa_alloc::init_allocator_with_default_settings;
    use kaspa_core::trace;
    use kaspa_math::Uint256;
    use workflow_perf_monitor::mem::get_process_memory_info;

    use crate::address::tracker::{Index, IndexSet, IndexVec, Indexer};

    use super::SubscriptionContext;

    fn create_addresses(count: usize) -> Vec<Address> {
        (0..count)
            .map(|i| Address::new(Prefix::Mainnet, kaspa_addresses::Version::PubKey, &Uint256::from_u64(i as u64).to_le_bytes()))
            .collect()
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = SubscriptionContext
    // (measuring IndexMap<ScriptPublicKey, u16>)
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM   MEM/ADDR
    // --------------------------------------------------
    // 10_000_000            5   1_098_920_755      110.0
    //  1_000_000           50     103_581_696      104.0
    //    100_000          100       9_157_836       91.6
    //     10_000        1_000         977_666       97.8
    //      1_000       10_000          94_633       94.6
    //        100      100_000           9_617       96.2
    //         10    1_000_000           1_325      132.5
    //          1   10_000_000             410      410.0
    fn test_subscription_context_size() {
        const ITEM_LEN: usize = 10_000_000;
        const NUM_ITEMS: usize = 5;

        init_allocator_with_default_settings();
        kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");

        trace!("Creating addresses...");
        let addresses = create_addresses(ITEM_LEN);

        let before = get_process_memory_info().unwrap();
        trace!("Creating contexts...");
        let context = (0..NUM_ITEMS).map(|_| SubscriptionContext::with_addresses(&addresses)).collect_vec();
        let after = get_process_memory_info().unwrap();

        trace!("Source addresses: {}", addresses.len());
        trace!("Context addresses: {}", context[0].address_tracker);
        trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / NUM_ITEMS as u64);
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = HashSet<u32>
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10      84'094'976        8.4
    //  1_000_000          100      10'524'508       10.5
    //    100_000        1_000         662_720        6.6
    //     10_000       10_000          86_369        8.6
    //      1_000      100_000          12_372       12.4
    //        100    1_000_000             821        8.2
    //         10   10_000_000             144       14.4
    //          1   10_000_000             112      112.0
    fn test_hash_set_u32_size() {
        const ITEM_LEN: usize = 10_000_000;
        const NUM_ITEMS: usize = 10;

        init_allocator_with_default_settings();
        kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");

        let before = get_process_memory_info().unwrap();
        trace!("Creating hash sets...");
        let sets = (0..NUM_ITEMS).map(|_| (0..ITEM_LEN as Index).rev().collect::<HashSet<_>>()).collect_vec();

        let after = get_process_memory_info().unwrap();
        trace!("Hash set length: {}", sets[0].len());
        trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / NUM_ITEMS as u64);
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = Vec<u32>
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10      40_208_384        4.0
    //  1_000_000          100       4_026_245        4.0
    //    100_000        1_000         403_791        4.0
    //     10_000       10_000          41_235        4.1
    //      1_000      100_000           4_141        4.1
    //        100    1_000_000             478        4.8
    //         10   10_000_000              72        7.2
    //          1   10_000_000              32       32.0
    fn test_vec_u32_size() {
        const ITEM_LEN: usize = 10_000_000;
        const NUM_ITEMS: usize = 10;

        init_allocator_with_default_settings();
        kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");

        let before = get_process_memory_info().unwrap();
        trace!("Creating vectors...");
        let sets = (0..NUM_ITEMS).map(|_| (0..ITEM_LEN as Index).collect::<Vec<_>>()).collect_vec();

        let after = get_process_memory_info().unwrap();
        trace!("Vector length: {}", sets[0].len());
        trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / NUM_ITEMS as u64);
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = IndexVec
    // (measuring Vec<u32>)
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10      47_033_139        4.7
    //  1_000_000          100       4_084_244        4.1
    //    100_000        1_000         405_450        4.1
    //     10_000       10_000          41_308        4.1
    //      1_000      100_000           4_146        4.1
    //        100    1_000_000             542        5.4
    //         10   10_000_000              88        8.8
    //          1   10_000_000              40       40.0
    fn test_index_vec_size() {
        const ITEM_LEN: usize = 10_000_000;
        const NUM_ITEMS: usize = 10;

        init_allocator_with_default_settings();
        kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");

        let before = get_process_memory_info().unwrap();
        trace!("Creating vectors...");
        let sets = (0..NUM_ITEMS)
            .map(|_| {
                // Rely on organic growth rather than pre-defined capacity
                let mut item = IndexVec::new(vec![]);
                (0..ITEM_LEN as Index).for_each(|x| {
                    item.insert(x);
                });
                item
            })
            .collect_vec();

        let after = get_process_memory_info().unwrap();
        trace!("Vector length: {}", sets[0].len());
        trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / NUM_ITEMS as u64);
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = IndexSet
    // (measuring HashSet<u32>)
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10      92_585_574        9.3
    //  1_000_000          100      10_661_683       10.7
    //    100_000        1_000         664_412        6.7
    //     10_000       10_000          86_439        8.6
    //      1_000      100_000          12_373       12.4
    //        100    1_000_000             822        8.2
    //         10   10_000_000             144       14.4
    //          1   10_000_000             112      112.0
    fn test_index_set_size() {
        const ITEM_LEN: usize = 1;
        const NUM_ITEMS: usize = 10_000_000;

        init_allocator_with_default_settings();
        kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");

        let before = get_process_memory_info().unwrap();
        trace!("Creating sets...");
        let sets = (0..NUM_ITEMS)
            .map(|_| {
                // Rely on organic growth rather than pre-defined capacity
                let mut item = IndexSet::new(vec![]);
                (0..ITEM_LEN as Index).for_each(|x| {
                    item.insert(x);
                });
                item
            })
            .collect_vec();

        let after = get_process_memory_info().unwrap();
        trace!("Set length: {}", sets[0].len());
        trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / NUM_ITEMS as u64);
    }
}
