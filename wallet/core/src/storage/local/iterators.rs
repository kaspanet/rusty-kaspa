use crate::imports::*;
use crate::iterator::*;
// use crate::storage::local::*;
use crate::storage::local::interface::LocalStore;
use crate::storage::*;
use async_trait::async_trait;

#[derive(Clone)]
struct StoreIteratorInner {
    store: Arc<LocalStore>,
    cursor: usize,
    chunk_size: usize,
}

impl std::fmt::Debug for StoreIteratorInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoreIteratorInner").field("cursor", &self.cursor).finish()
    }
}

static DEFAULT_CHUNK_SIZE: usize = 25;
#[macro_export]
macro_rules! declare_iterator {
    ($prop:ident, $name:ident, $id : ty, $data : ty) => {
        #[derive(Clone, Debug)]
        pub struct $name(StoreIteratorInner);

        impl $name {
            pub fn new(store: Arc<LocalStore>, iterator_options: IteratorOptions) -> Self {
                Self(StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) })
            }
        }

        #[async_trait]
        impl Iterator for $name {
            type Item = $id;

            async fn next(&mut self) -> Option<Vec<Self::Item>> {
                let vec = &self.0.store.cache.inner().$prop.vec;
                if self.0.cursor >= vec.len() {
                    return None;
                } else {
                    let slice = &vec[self.0.cursor as usize..(self.0.cursor as usize + self.0.chunk_size).min(vec.len())];
                    if slice.is_empty() {
                        None
                    } else {
                        self.0.cursor += slice.len();
                        let vec = slice.iter().map(|data| data.id.clone()).collect();
                        Some(vec)
                    }
                }
            }
        }
    };
}

declare_iterator!(accounts, AccountIterator, AccountId, Account);
declare_iterator!(metadata, MetadataIterator, AccountId, Metadata);
declare_iterator!(transaction_records, TransactionRecordIterator, TransactionRecordId, TransactionRecord);

pub struct PrvKeyDataIterator(StoreIteratorInner);

impl PrvKeyDataIterator {
    pub fn new(store: Arc<LocalStore>, iterator_options: IteratorOptions) -> Self {
        Self(StoreIteratorInner { store, cursor: 0, chunk_size: iterator_options.chunk_size.unwrap_or(DEFAULT_CHUNK_SIZE) })
    }
}

#[async_trait]
impl Iterator for PrvKeyDataIterator {
    type Item = Arc<PrvKeyDataInfo>;

    async fn next(&mut self) -> Option<Vec<Self::Item>> {
        let vec = &self.0.store.cache.inner().prv_key_data_info;
        if self.0.cursor >= vec.len() {
            return None;
        } else {
            let slice = &vec[self.0.cursor..(self.0.cursor + self.0.chunk_size)];
            if slice.is_empty() {
                None
            } else {
                self.0.cursor += slice.len();
                Some(slice.to_vec())
            }
        }
    }
}
