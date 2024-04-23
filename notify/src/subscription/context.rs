use crate::{
    address::tracker::Tracker,
    listener::ListenerId,
    subscription::{
        single::{UtxosChangedState, UtxosChangedSubscription},
        DynSubscription,
    },
};
use std::{ops::Deref, sync::Arc};

#[cfg(test)]
use kaspa_addresses::Address;

#[derive(Debug)]
pub struct SubscriptionContextInner {
    pub address_tracker: Tracker,
    pub utxos_changed_subscription_to_all: DynSubscription,
}

impl SubscriptionContextInner {
    const CONTEXT_LISTENER_ID: ListenerId = ListenerId::MAX;

    pub fn new() -> Self {
        Self::with_options(None)
    }

    pub fn with_options(max_addresses: Option<usize>) -> Self {
        let address_tracker = Tracker::new(max_addresses);
        let utxos_changed_subscription_all =
            Arc::new(UtxosChangedSubscription::new(UtxosChangedState::All, Self::CONTEXT_LISTENER_ID));
        Self { address_tracker, utxos_changed_subscription_to_all: utxos_changed_subscription_all }
    }

    #[cfg(test)]
    pub fn with_addresses(addresses: &[Address]) -> Self {
        let address_tracker = Tracker::with_addresses(addresses);
        let utxos_changed_subscription_all =
            Arc::new(UtxosChangedSubscription::new(UtxosChangedState::All, Self::CONTEXT_LISTENER_ID));
        Self { address_tracker, utxos_changed_subscription_to_all: utxos_changed_subscription_all }
    }
}

impl Default for SubscriptionContextInner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Default)]
pub struct SubscriptionContext {
    inner: Arc<SubscriptionContextInner>,
}

impl SubscriptionContext {
    pub fn new() -> Self {
        Self::with_options(None)
    }

    pub fn with_options(max_addresses: Option<usize>) -> Self {
        let inner = Arc::new(SubscriptionContextInner::with_options(max_addresses));
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
    use crate::{
        address::tracker::{CounterMap, Index, IndexSet, Indexer, RefCount},
        subscription::SubscriptionContext,
    };
    use itertools::Itertools;
    use kaspa_addresses::{Address, Prefix};
    use kaspa_alloc::init_allocator_with_default_settings;
    use kaspa_core::trace;
    use kaspa_math::Uint256;
    use std::collections::{HashMap, HashSet};
    use workflow_perf_monitor::mem::get_process_memory_info;

    fn create_addresses(count: usize) -> Vec<Address> {
        (0..count)
            .map(|i| Address::new(Prefix::Mainnet, kaspa_addresses::Version::PubKey, &Uint256::from_u64(i as u64).to_le_bytes()))
            .collect()
    }

    fn measure_consumed_memory<T, F: FnOnce() -> Vec<T>, F2: FnOnce(&T) -> (usize, usize)>(
        item_len: usize,
        num_items: usize,
        ctor: F,
        length_and_capacity: F2,
    ) -> Vec<T> {
        let before = get_process_memory_info().unwrap();

        trace!("Creating items...");
        let items = ctor();

        let after = get_process_memory_info().unwrap();

        trace!("Required item length: {}", item_len);
        trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / num_items as u64);
        trace!(
            "Memory/idx: {}",
            ((after.resident_set_size - before.resident_set_size) as f64 / num_items as f64 / item_len as f64 * 10.0).round() / 10.0
        );

        let (len, capacity) = length_and_capacity(&items[0]);
        match len > 0 {
            true => trace!(
                "Actual item: len = {}, capacity = {}, free space = +{:.1}%",
                len,
                capacity,
                (capacity - len) as f64 * 100.0 / len as f64
            ),
            false => trace!("Actual item: len = {}, capacity = {}", len, capacity),
        }

        items
    }

    fn init_and_measure_consumed_memory<T, F: FnOnce() -> Vec<T>, F2: FnOnce(&T) -> (usize, usize)>(
        item_len: usize,
        num_items: usize,
        ctor: F,
        length_and_capacity: F2,
    ) -> Vec<T> {
        init_allocator_with_default_settings();
        kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");
        measure_consumed_memory(item_len, num_items, ctor, length_and_capacity)
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = SubscriptionContext
    // (measuring IndexMap<ScriptPublicKey, u16>)
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM   MEM/ADDR
    // --------------------------------------------------
    // 10_000_000            5   1_098_744_627      109.9
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

        let _ = measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || (0..NUM_ITEMS).map(|_| SubscriptionContext::with_addresses(&addresses)).collect_vec(),
            |x| (x.address_tracker.len(), x.address_tracker.capacity()),
        );
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = HashMap<u32, u16>
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10     151_214_489       15.1
    //  1_000_000          100      18_926_059       18.9
    //    100_000        1_000       1_187_864       11.9
    //     10_000       10_000         152_063       15.2
    //      1_000      100_000          20_576       20.6
    //        100    1_000_000           1_336       13.4
    //         10   10_000_000             241       24.1
    //          1   10_000_000             128      128.4
    fn test_hash_map_u32_u16_size() {
        const ITEM_LEN: usize = 1;
        const NUM_ITEMS: usize = 10_000_000;

        let _ = init_and_measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || {
                (0..NUM_ITEMS)
                    .map(|_| (0..ITEM_LEN as Index).map(|i| (i, (ITEM_LEN as Index - i) as RefCount)).rev().collect::<HashMap<_, _>>())
                    .collect_vec()
            },
            |x| (x.len(), x.capacity()),
        );
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = CounterMap
    // (measuring HashMap<u32, u16>)
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10     151_239_065       15.1
    //  1_000_000          100      18_927_534       18.9
    //    100_000        1_000       1_188_024       11.9
    //     10_000       10_000         152_077       15.2
    //      1_000      100_000          20_587       20.6
    //        100    1_000_000           1_344       13.4
    //         10   10_000_000             249       24.9
    //          1   10_000_000             136      136.5
    fn test_counter_map_size() {
        const ITEM_LEN: usize = 10;
        const NUM_ITEMS: usize = 10_000_000;

        let _ = init_and_measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || {
                (0..NUM_ITEMS)
                    .map(|_| {
                        // Reserve the required capacity
                        // Note: the resulting allocated HashMap bucket count is (capacity * 8 / 7).next_power_of_two()
                        let mut item = CounterMap::with_capacity(ITEM_LEN);

                        (0..ITEM_LEN as Index).for_each(|x| {
                            item.insert(x);
                        });
                        item
                    })
                    .collect_vec()
            },
            |x| (x.len(), x.capacity()),
        );
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
        const ITEM_LEN: usize = 1_000_000;
        const NUM_ITEMS: usize = 100;

        let _ = init_and_measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || (0..NUM_ITEMS).map(|_| (0..ITEM_LEN as Index).rev().collect::<HashSet<_>>()).collect_vec(),
            |x| (x.len(), x.capacity()),
        );
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = HashSet<u32> emptied
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
    fn test_emptied_hash_set_u32_size() {
        const ITEM_LEN: usize = 1_000_000;
        const NUM_ITEMS: usize = 100;

        let _ = init_and_measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || {
                (0..NUM_ITEMS)
                    .map(|_| {
                        let mut set = (0..ITEM_LEN as Index).rev().collect::<HashSet<_>>();
                        let original_capacity = set.capacity();
                        let _ = set.drain();
                        assert!(set.is_empty());
                        assert_eq!(original_capacity, set.capacity());
                        set
                    })
                    .collect_vec()
            },
            |x| (x.len(), x.capacity()),
        );
    }

    #[test]
    #[ignore = "measuring consumed memory"]
    // ITEM = IndexSet
    // (measuring HashSet<u32>)
    //
    //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // --------------------------------------------------
    // 10_000_000           10      84_119_961        8.4
    //  1_000_000          100      10_526_720       10.5
    //    100_000        1_000         662_974        6.6
    //     10_000       10_000          86_424        8.6
    //      1_000      100_000          12_381       12.4
    //        100    1_000_000             830        8.3
    //         10   10_000_000             152       15.2
    //          1   10_000_000             120      120.0
    fn test_index_set_size() {
        const ITEM_LEN: usize = 10_000_000;
        const NUM_ITEMS: usize = 10;

        let _ = init_and_measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || {
                (0..NUM_ITEMS)
                    .map(|_| {
                        // Reserve the required capacity
                        // Note: the resulting allocated HashSet bucket count is (capacity * 8 / 7).next_power_of_two()
                        let mut item = IndexSet::with_capacity(ITEM_LEN);

                        (0..ITEM_LEN as Index).for_each(|x| {
                            item.insert(x);
                        });
                        item
                    })
                    .collect_vec()
            },
            |x| (x.len(), x.capacity()),
        );
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

        let _ = init_and_measure_consumed_memory(
            ITEM_LEN,
            NUM_ITEMS,
            || (0..NUM_ITEMS).map(|_| (0..ITEM_LEN as Index).collect::<Vec<_>>()).collect_vec(),
            |x| (x.len(), x.capacity()),
        );
    }
    // #[test]
    // #[ignore = "measuring consumed memory"]
    // // ITEM = DashSet
    // // (measuring DashSet<u32>)
    // //
    // //   ITEM_LEN    NUM_ITEMS     MEMORY/ITEM    MEM/IDX
    // // --------------------------------------------------
    // // 10_000_000           10      96_439_500        9.6
    // //  1_000_000          100      11_942_010       11.9
    // //    100_000        1_000         826_400        8.3
    // //     10_000       10_000         107_060       10.7
    // //      1_000      100_000          19_114       19.1
    // //        100    1_000_000          12_717      127.2
    // //         10    1_000_000           8_865      886.5
    // //          1    1_000_000           8_309     8309.0
    // fn test_dash_set_size() {
    //     const ITEM_LEN: usize = 1;
    //     const NUM_ITEMS: usize = 1_000_000;

    //     init_allocator_with_default_settings();
    //     kaspa_core::log::try_init_logger("INFO,kaspa_notify::subscription::context=trace");

    //     let before = get_process_memory_info().unwrap();
    //     trace!("Creating sets...");
    //     let sets = (0..NUM_ITEMS)
    //         .map(|_| {
    //             // Rely on organic growth rather than pre-defined capacity
    //             let item = DashSet::new();
    //             (0..ITEM_LEN as Index).for_each(|x| {
    //                 item.insert(x);
    //             });
    //             item
    //         })
    //         .collect_vec();

    //     let after = get_process_memory_info().unwrap();
    //     trace!("Set length: {}", sets[0].len());
    //     trace!("Memory consumed: {}", (after.resident_set_size - before.resident_set_size) / NUM_ITEMS as u64);
    // }
}
