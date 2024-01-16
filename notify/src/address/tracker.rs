use std::{ops::Deref, slice::Iter};

use indexmap::{map::Entry, IndexMap};
use kaspa_addresses::{Address, Prefix};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_txscript::{extract_script_pub_key_address, pay_to_address_script};
use parking_lot::RwLock;

pub type AddressIndex = u32;
pub type RefCount = u16;

pub struct AddressIndexes(Vec<AddressIndex>);

impl AddressIndexes {
    pub fn new(mut indexes: Vec<AddressIndex>) -> Self {
        indexes.sort();
        Self(indexes)
    }

    pub fn contains(&self, address_idx: AddressIndex) -> bool {
        self.0.binary_search(&address_idx).is_ok()
    }

    pub(self) fn insert(&mut self, address_idx: AddressIndex) -> bool {
        match self.0.binary_search(&address_idx) {
            Ok(_) => false,
            Err(index) => {
                self.0.insert(index, address_idx);
                true
            }
        }
    }

    pub(self) fn remove(&mut self, address_idx: AddressIndex) -> bool {
        match self.0.binary_search(&address_idx) {
            Ok(index) => {
                self.0.remove(index);
                true
            }
            Err(_) => false,
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, AddressIndex> {
        self.0.iter()
    }
}

impl Deref for AddressIndexes {
    type Target = [AddressIndex];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<AddressIndex>> for AddressIndexes {
    fn from(item: Vec<AddressIndex>) -> Self {
        Self::new(item)
    }
}

#[derive(Debug)]
struct Inner {
    script_pub_keys: IndexMap<ScriptPublicKey, RefCount>,
}

impl Inner {
    fn new() -> Self {
        Self { script_pub_keys: IndexMap::new() }
    }

    fn with_capacity(capacity: usize) -> Self {
        Self { script_pub_keys: IndexMap::with_capacity(capacity) }
    }

    fn get(&self, spk: &ScriptPublicKey) -> Option<(AddressIndex, RefCount)> {
        self.script_pub_keys.get_full(spk).map(|(index, _, count)| (index as AddressIndex, *count))
    }

    // fn get_address(&self, address: &Address) -> Option<(AddressIndex, RefCount)> {
    //     let spk = pay_to_address_script(address);
    //     self.script_pub_keys.get_full(&spk).map(|(index, _, count)| (index as AddressIndex, *count))
    // }

    // fn get_index(&self, address_idx: AddressIndex) -> Option<RefCount> {
    //     self.script_pub_keys.get_index(address_idx as usize).map(|(_, count)| *count)
    // }

    fn get_index_address(&self, address_idx: AddressIndex, prefix: Prefix) -> Option<Address> {
        self.script_pub_keys
            .get_index(address_idx as usize)
            .map(|(spk, _)| extract_script_pub_key_address(spk, prefix).expect("is retro-convertible"))
    }

    fn get_or_insert(&mut self, spk: ScriptPublicKey) -> AddressIndex {
        // TODO: reuse entries with counter at 0 when available and some map size threshold is reached
        match self.script_pub_keys.entry(spk) {
            Entry::Occupied(entry) => entry.index() as AddressIndex,
            Entry::Vacant(entry) => {
                let index = entry.index() as AddressIndex;
                let _ = *entry.insert(0);
                index
            }
        }
    }

    fn inc_count(&mut self, address_idx: AddressIndex) {
        if let Some((_, count)) = self.script_pub_keys.get_index_mut(address_idx as usize) {
            *count += 1;
        }
    }

    fn dec_count(&mut self, address_idx: AddressIndex) {
        if let Some((_, count)) = self.script_pub_keys.get_index_mut(address_idx as usize) {
            if *count == 0 {
                panic!("Address tracker is trying to decrease an address counter that is already at zero");
            }
            *count -= 1
        }
    }

    // fn register_address(&mut self, address: &Address) -> AddressIndex {
    //     let spk = pay_to_address_script(address);
    //     let index = match self.script_pub_keys.get_full_mut(&spk) {
    //         Some((index, _, count)) => {
    //             *count += 1;
    //             index
    //         }
    //         None => {
    //             // TODO: reuse entries with counter at 0 when available and some map size is reached
    //             self.script_pub_keys.insert_full(spk, 1).0
    //         }
    //     };
    //     index as AddressIndex
    // }

    // fn unregister_address(&mut self, address: &Address) -> Option<AddressIndex> {
    //     let spk = pay_to_address_script(address);
    //     self.script_pub_keys.get_full_mut(&spk).map(|(index, _, count)| {
    //         *count -= 1;
    //         index as AddressIndex
    //     })
    // }
}

/// Tracker of multiple [`Address`](kaspa_addresses::Address), indexing and counting registrations
#[derive(Debug)]
pub struct AddressTracker {
    inner: RwLock<Inner>,
}

impl AddressTracker {
    const ADDRESS_CHUNK_SIZE: usize = 256;

    pub fn new() -> Self {
        Self { inner: RwLock::new(Inner::new()) }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { inner: RwLock::new(Inner::with_capacity(capacity)) }
    }

    pub fn get(&self, spk: &ScriptPublicKey) -> Option<(AddressIndex, RefCount)> {
        self.inner.read().get(spk)
    }

    // pub fn get_address(&self, address: &Address) -> Option<(AddressIndex, RefCount)> {
    //     self.inner.read().get_address(address)
    // }

    // pub fn get_index(&self, address_idx: AddressIndex) -> Option<RefCount> {
    //     self.inner.read().get_index(address_idx)
    // }

    pub fn get_index_address(&self, address_idx: AddressIndex, prefix: Prefix) -> Option<Address> {
        self.inner.read().get_index_address(address_idx, prefix)
    }

    pub fn contains(&self, indexes: &AddressIndexes, spk: &ScriptPublicKey) -> bool {
        if let Some((address_idx, _)) = self.inner.read().get(spk) {
            indexes.contains(address_idx)
        } else {
            false
        }
    }

    pub fn register(&mut self, indexes: &mut AddressIndexes, addresses: &[Address]) -> Vec<Address> {
        let mut added = Vec::with_capacity(addresses.len());
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            for address in chunk {
                let spk = pay_to_address_script(address);
                let address_idx = inner.get_or_insert(spk);
                if indexes.insert(address_idx) {
                    added.push(address.clone());
                    inner.inc_count(address_idx);
                }
            }
        }
        added
    }

    pub fn unregister(&mut self, indexes: &mut AddressIndexes, addresses: &[Address]) -> Vec<Address> {
        let mut removed = Vec::with_capacity(addresses.len());
        for chunk in addresses.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let mut inner = self.inner.write();
            for address in chunk {
                let spk = pay_to_address_script(address);
                if let Some((address_idx, _)) = inner.get(&spk) {
                    if indexes.remove(address_idx) {
                        removed.push(address.clone());
                        inner.dec_count(address_idx);
                    }
                }
            }
        }
        removed
    }

    pub fn to_addresses(&self, indexes: &[AddressIndex], prefix: Prefix) -> Vec<Address> {
        let mut addresses = Vec::with_capacity(indexes.len());
        for chunk in indexes.chunks(Self::ADDRESS_CHUNK_SIZE) {
            let inner = self.inner.read();
            chunk.iter().for_each(|address_idx| {
                if let Some(address) = inner.get_index_address(*address_idx, prefix) {
                    addresses.push(address);
                }
            });
        }
        addresses
    }
}

impl Default for AddressTracker {
    fn default() -> Self {
        Self::new()
    }
}
